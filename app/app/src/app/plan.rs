//! Semantic plan → Splash DSL.
//!
//! The generating model emits a small typed PLAN; this lowers it to the card. It
//! is the runtime half of separating intent from realization.
//!
//! ## Why
//!
//! When the model emits the whole card, the error surface is the whole card. On
//! 2026-07-29/30 that produced six distinct silent failures in one domain —
//! invented coordinates (71 per card), a guessed temperature range that flattened
//! every gradient, weekday names off by one in *every* card ever generated, tofu
//! boxes where CJK should be, a fixed root height that truncated half the card,
//! and a stray sibling node that squeezed it into half the width. Each was fixed
//! by adding a prohibition to `a2app/apps/weather/app.md`, which is now 448 lines
//! of which 28 are MUST/NEVER rules. That is whack-a-mole with a scar log.
//!
//! Every one of those fixes had the same shape: NARROW WHAT THE MODEL MAY WRITE and
//! move the decision into the runtime. This module does that wholesale rather than
//! one bug at a time.
//!
//! ## The rule it enforces
//!
//! **The runtime must never accept a value it cannot verify.** In order of
//! preference:
//!
//! 1. The field does not exist and the runtime derives it. A `place` is a NAME, so
//!    a coordinate is unexpressible; the week's extent is not an input at all.
//! 2. The field is typed and a violation is REJECTED before lowering, where the
//!    repair loop can act on a precise message.
//! 3. The model is asked to get it right. This is what a 448-line spec was doing,
//!    and it is what this replaces.
//!
//! Recovery matters as much as prevention: one bad field in 16 KB of free-form DSL
//! means regenerating the whole card — nondeterministically different elsewhere —
//! whereas an invalid plan field is rejected at the field. The equivalent plan is
//! ~600 bytes against ~16 KB of card, so the surface is ~27x smaller and typed.
//!
//! ## Scope
//!
//! Weather only, deliberately. It is the domain whose failure modes are documented
//! and whose lowered output can be diffed against a card verified on device. A
//! stocks plan lowers in the prototype (`docs/prototype-semantic-plan/`) but needs
//! state/actions/views, which want the identity bridge in
//! `docs/CARD-STATE-IDENTITY.md` first.

use serde::Deserialize;

/// A plan the model emitted. Unknown fields are REJECTED rather than ignored:
/// silently dropping a field a card asked for is the failure mode this exists to
/// remove.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub plan: String,
    #[serde(default)]
    pub locale: String,
    pub place: PlaceRef,
    /// A photo search phrase. The model's own words — it cannot be "wrong", so
    /// tier 3 is acceptable here.
    #[serde(default)]
    pub photo: String,
    pub sections: Vec<Section>,
}

/// A place by NAME. There is deliberately no latitude or longitude field: a
/// coordinate recalled by a model is an invented number exactly like a recalled
/// temperature — plausible for a famous city, fabricated anywhere else, and
/// silently the wrong place when a name is ambiguous. The runtime geocodes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceRef {
    pub query: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub block: String,
    #[serde(default)]
    pub args: Args,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Current condition, as a WORD not a shader index. `draw_bg.cond: 2` is a
    /// backend detail and an unverifiable number; "cloudy" is intent.
    #[serde(default)]
    pub condition: String,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default)]
    pub days: Option<u32>,
    #[serde(default)]
    pub tiles: Vec<String>,
}

/// A condition word → the WeatherIcon shader index and a forecast-row emoji.
/// Closed set: an unknown word is rejected, never silently drawn as clear sky.
fn condition_icon(c: &str) -> Option<(u32, &'static str)> {
    Some(match c {
        "clear" => (0, "☀️"),
        "partly_cloudy" => (1, "⛅"),
        "cloudy" => (2, "☁️"),
        "rain" => (3, "🌧️"),
        "thunderstorm" => (4, "⛈️"),
        "snow" => (5, "❄️"),
        "wind" => (6, "🌬️"),
        "fog" => (7, "🌫️"),
        _ => return None,
    })
}

/// The condition word rendered for display, per locale.
fn condition_word(c: &str, zh: bool) -> &'static str {
    match (c, zh) {
        ("clear", false) => "Clear",
        ("partly_cloudy", false) => "Partly Cloudy",
        ("cloudy", false) => "Cloudy",
        ("rain", false) => "Rain",
        ("thunderstorm", false) => "Thunderstorm",
        ("snow", false) => "Snow",
        ("wind", false) => "Wind",
        ("fog", false) => "Fog",
        ("clear", true) => "晴",
        ("partly_cloudy", true) => "局部多云",
        ("cloudy", true) => "多云",
        ("rain", true) => "雨",
        ("thunderstorm", true) => "雷暴",
        ("snow", true) => "雪",
        ("wind", true) => "大风",
        ("fog", true) => "雾",
        _ => "",
    }
}

/// Tile key → (caption_en, caption_zh, helper, path, unit).
fn tile_spec(k: &str) -> Option<(&'static str, &'static str, &'static str, &'static str, &'static str)> {
    Some(match k {
        "aqi" => ("AIR QUALITY", "空气质量", "airquality", "current.us_aqi", ""),
        "uv" => ("UV INDEX", "紫外线", "weather", "daily.uv_index_max.0", ""),
        "humidity" => ("HUMIDITY", "湿度", "weather", "current.relative_humidity_2m", "%"),
        "wind" => ("WIND", "风速", "weather", "current.wind_speed_10m", " km/h"),
        "pressure" => ("PRESSURE", "气压", "weather", "current.surface_pressure", " hPa"),
        _ => return None,
    })
}

/// Everything the plan does NOT carry, because the runtime owns it. Changing a
/// value here restyles every card with no model call — which is the other half of
/// the point: a look change costs a token edit, not a 45-second regeneration that
/// may alter unrelated things.
mod theme {
    pub const BASE: &str = "#0a0e14";
    pub const SCRIM: &str = "#00000066";
    pub const PANEL: &str = "#00000055";
    pub const TILE: &str = "#ffffff1f";
    pub const TEXT: &str = "#ffffff";
    pub const TEXT_SOFT: &str = "#ffffffe6";
    pub const TEXT_DIM: &str = "#ffffffb3";
    pub const TEXT_FAINT: &str = "#ffffff99";
    pub const ROW_DIM: &str = "#ffffff88";
    /// A FIXED height taller than the content. A `Fit` Overlay takes its tallest
    /// child, so a photo shorter than the column ends in a hard edge of bare BASE
    /// partway down the card.
    pub const PHOTO_H: u32 = 2000;
    pub const PAGE_PAD: &str = "Inset{left: 22 top: 54 right: 22 bottom: 8}";
    pub const PANEL_RADIUS: &str = "20.0";
    pub const TILE_RADIUS: &str = "18.0";
    pub const ROW_H: u32 = 40;
    pub const DAY_W: u32 = 92;
    pub const ICON_W: u32 = 34;
    pub const TEMP_W: u32 = 46;
    /// A right-aligned label sets its text flush to the box edge and `°` overhangs
    /// the clip, rendering as `29ᶜ`. Widening the box does not help — alignment
    /// moves the text with the edge. This padding pulls the digits back inside AND
    /// makes the gaps either side of the bar equal.
    pub const TEMP_PAD: u32 = 5;
    pub const BAR_MARGIN: u32 = 10;
    pub const BAR_H: u32 = 8;
    pub const GAP: u32 = 16;
    pub const MAP_H: u32 = 190;
}

/// Lower a plan to Splash DSL, or explain why it cannot be lowered.
///
/// The error strings are written for the repair loop: they name the offending
/// field and the permitted values, so a retry is targeted rather than a whole-card
/// regeneration.
pub fn lower_plan(json: &str) -> Result<String, String> {
    let plan: Plan = serde_json::from_str(json)
        .map_err(|e| format!("plan is not valid: {e}"))?;
    if plan.plan != "weather" {
        return Err(format!(
            "unsupported plan kind {:?} — only \"weather\" lowers today",
            plan.plan
        ));
    }
    if plan.place.query.trim().is_empty() {
        return Err("place.query is empty — name the place".to_string());
    }
    let zh = plan.locale.starts_with("zh");
    let place = &plan.place.query;
    // Coordinates are RESOLVED, never carried. Called inline at each use site:
    // a card's top-level `let` bindings evaluate once at build time, before any
    // fetch resolves, so hoisting this would freeze it at the -9999 sentinel.
    let lat = format!("sys.geocodenum({:?}, \"lat\")", place);
    let lon = format!("sys.geocodenum({:?}, \"lon\")", place);
    let ll = format!("{lat}, {lon}");

    let mut body = String::new();
    for (i, sec) in plan.sections.iter().enumerate() {
        let s = match sec.block.as_str() {
            "CurrentConditions" => current_conditions(&sec.args, &ll, place, zh, i)?,
            "Forecast" => forecast(&sec.args, &ll, zh, i)?,
            "AirQualityField" => air_quality_field(&ll, &lat, &lon, zh),
            "SunMoon" => sun_moon(&ll, zh),
            "Details" => details(&sec.args, &ll, zh, i)?,
            other => {
                return Err(format!(
                    "sections[{i}]: unknown block {other:?} — permitted: \
                     CurrentConditions, Forecast, AirQualityField, SunMoon, Details"
                ))
            }
        };
        body.push_str(&s);
        body.push('\n');
    }

    // EXACTLY ONE top-level node, by construction. Sibling top-level nodes lay out
    // SIDE BY SIDE, so an extra background node does not sit behind the card — it
    // takes half the width and squeezes the card into the other half.
    Ok(format!(
        "// name: weather-app\n\
         // LOWERED from a semantic plan — do not edit.\n\
         SolidView{{ width: Fill height: Fit flow: Overlay new_batch: true draw_bg.color: {base}\n\
         \x20   Image{{ src: http_resource(sys.photo({photo:?})) fit: ImageFit.CropToFill width: Fill height: {ph} }}\n\
         \x20   SolidView{{ width: Fill height: Fill draw_bg.color: {scrim} }}\n\
         \x20   View{{ width: Fill height: Fit flow: Down padding: {pad}\n\
         {body}\x20   }}\n\
         }}\n",
        base = theme::BASE,
        photo = if plan.photo.trim().is_empty() { place.clone() } else { plan.photo.clone() },
        ph = theme::PHOTO_H,
        scrim = theme::SCRIM,
        pad = theme::PAGE_PAD,
        body = body,
    ))
}

fn current_conditions(a: &Args, ll: &str, place: &str, zh: bool, i: usize) -> Result<String, String> {
    let (cond, _) = condition_icon(&a.condition).ok_or_else(|| {
        format!(
            "sections[{i}] CurrentConditions: condition {:?} is not one of \
             clear, partly_cloudy, cloudy, rain, thunderstorm, snow, wind, fog",
            a.condition
        )
    })?;
    let word = condition_word(&a.condition, zh);
    Ok(format!(
        "\x20       View{{ width: Fill height: Fit flow: Down align: Align{{x: 0.5}}\n\
         \x20           TextTitle{{ text: sys.geocode({place:?}, \"name\") draw_text.color: {soft} }}\n\
         \x20           TextHero{{ text: sys.weather({ll}, \"current.temperature_2m\") + \"°\" \
         margin: Inset{{top: 2 bottom: 0}} draw_text.color: {text} }}\n\
         \x20           View{{ width: Fit height: 52 flow: Right align: Align{{x: 0.5 y: 0.5}} spacing: 8\n\
         \x20               WeatherIcon{{ draw_bg.cond: {cond} width: 46 height: 46 }}\n\
         \x20               TextBody{{ text: {word:?} draw_text.color: {soft} }}\n\
         \x20           }}\n\
         \x20           TextStat{{ text: \"↑\" + sys.weather({ll}, \"daily.temperature_2m_max.0\") + \"°   ↓\" \
         + sys.weather({ll}, \"daily.temperature_2m_min.0\") + \"°   ≈\" \
         + sys.weather({ll}, \"current.apparent_temperature\") + \"°\" draw_text.color: {dim} }}\n\
         \x20       }}",
        soft = theme::TEXT_SOFT,
        text = theme::TEXT,
        dim = theme::TEXT_DIM,
    ))
}

fn forecast(a: &Args, ll: &str, zh: bool, i: usize) -> Result<String, String> {
    let days = a.days.unwrap_or(7).clamp(1, 7) as usize;
    let loc = if zh { "zh" } else { "en" };
    let mut rows = String::new();
    for d in 0..days {
        // An absent per-day condition falls back to cloudy rather than failing the
        // whole card: the icon is decoration, and a missing one must not cost the
        // user their forecast.
        let word = a.conditions.get(d).map(String::as_str).unwrap_or("cloudy");
        let (_, emoji) = condition_icon(word).ok_or_else(|| {
            format!("sections[{i}] Forecast: conditions[{d}] {word:?} is not a known condition")
        })?;
        rows.push_str(&format!(
            "\x20           View{{ width: Fill height: {rh} flow: Right align: Align{{y: 0.5}}\n\
             \x20               TextRow{{ width: {dw} text: sys.dayname({ll}, {d}, {loc:?}) draw_text.color: {soft} }}\n\
             \x20               Label{{ width: {iw} text: {emoji:?} draw_text.text_style.font_size: 16 }}\n\
             \x20               TextRow{{ width: {tw} align: Align{{x: 1.0}} padding: Inset{{right: {tp}}} \
             text: sys.weather({ll}, \"daily.temperature_2m_min.{d}\") + \"°\" draw_text.color: {rowdim} }}\n\
             \x20               TempBar{{ width: Fill height: {bh} margin: Inset{{left: {bm} right: {bm}}} \
             draw_bg.tlo: sys.weathernum({ll}, \"daily.temperature_2m_min.{d}\") \
             draw_bg.thi: sys.weathernum({ll}, \"daily.temperature_2m_max.{d}\") \
             draw_bg.wmin: sys.weekmin({ll}) draw_bg.wmax: sys.weekmax({ll}) }}\n\
             \x20               TextRow{{ width: {tw} align: Align{{x: 0.0}} padding: Inset{{left: {tp}}} \
             text: sys.weather({ll}, \"daily.temperature_2m_max.{d}\") + \"°\" draw_text.color: {text} }}\n\
             \x20           }}\n",
            rh = theme::ROW_H,
            dw = theme::DAY_W,
            iw = theme::ICON_W,
            tw = theme::TEMP_W,
            tp = theme::TEMP_PAD,
            bh = theme::BAR_H,
            bm = theme::BAR_MARGIN,
            soft = theme::TEXT_SOFT,
            rowdim = theme::ROW_DIM,
            text = theme::TEXT,
        ));
    }
    Ok(format!(
        "\x20       RoundedView{{ width: Fill height: Fit flow: Down new_batch: true \
         draw_bg.color: {panel} draw_bg.border_radius: {pr} margin: Inset{{top: {gap}}} \
         padding: Inset{{left: 16 right: 16 top: 10 bottom: 10}}\n{rows}\x20       }}",
        panel = theme::PANEL,
        pr = theme::PANEL_RADIUS,
        gap = theme::GAP,
    ))
}

fn air_quality_field(ll: &str, lat: &str, lon: &str, zh: bool) -> String {
    let cap = if zh { "空气质量图" } else { "Air Quality" };
    format!(
        "\x20       TextCaption{{ text: {cap:?} draw_text.color: {faint} margin: Inset{{top: {gap} bottom: 6}} }}\n\
         \x20       View{{ width: Fill height: {mh} flow: Overlay\n\
         \x20           Image{{ src: http_resource(sys.basemap({ll})) fit: ImageFit.CropToFill width: Fill height: {mh} }}\n\
         \x20           AqiContour{{ width: Fill height: {mh} lat: {lat} lon: {lon} span: 1.6 }}\n\
         \x20       }}",
        faint = theme::TEXT_FAINT,
        gap = theme::GAP,
        mh = theme::MAP_H,
    )
}

fn sun_moon(ll: &str, zh: bool) -> String {
    let (cap, illum, namef) = if zh {
        ("日出 / 日落", "% 照亮", "name_zh")
    } else {
        ("Sunrise / Sunset", "% illuminated", "name")
    };
    format!(
        "\x20       RoundedView{{ width: Fill height: Fit flow: Down new_batch: true \
         draw_bg.color: {panel} draw_bg.border_radius: {pr} margin: Inset{{top: {gap}}} \
         padding: Inset{{left: 16 right: 16 top: 14 bottom: 14}} spacing: 10\n\
         \x20           TextCaption{{ text: {cap:?} draw_text.color: {faint} }}\n\
         \x20           SunArc{{ width: Fill height: 96 draw_bg.progress: sys.daylight({ll}) }}\n\
         \x20           View{{ width: Fill height: Fit flow: Right\n\
         \x20               TextRow{{ text: sys.weather({ll}, \"daily.sunrise.0\") draw_text.color: {dim} }}\n\
         \x20               Filler{{}}\n\
         \x20               TextRow{{ text: sys.weather({ll}, \"daily.sunset.0\") draw_text.color: {dim} }}\n\
         \x20           }}\n\
         \x20           View{{ width: Fill height: Fit flow: Right align: Align{{y: 0.5}} spacing: 14 margin: Inset{{top: 8}}\n\
         \x20               MoonPhase{{ width: 72 height: 72 draw_bg.phase: sys.moonnum(\"phase\") }}\n\
         \x20               View{{ width: Fill height: Fit flow: Down spacing: 4\n\
         \x20                   TextBody{{ text: sys.moonphase({namef:?}) draw_text.color: {soft} }}\n\
         \x20                   TextCaption{{ text: sys.moonphase(\"illumination\") + {illum:?} draw_text.color: {faint} }}\n\
         \x20               }}\n\
         \x20           }}\n\
         \x20       }}",
        panel = theme::PANEL,
        pr = theme::PANEL_RADIUS,
        gap = theme::GAP,
        faint = theme::TEXT_FAINT,
        dim = theme::TEXT_DIM,
        soft = theme::TEXT_SOFT,
    )
}

fn details(a: &Args, ll: &str, zh: bool, i: usize) -> Result<String, String> {
    if a.tiles.is_empty() {
        return Err(format!("sections[{i}] Details: tiles is empty — name at least two"));
    }
    let mut cells: Vec<String> = Vec::new();
    for k in &a.tiles {
        let (cap_en, cap_zh, helper, path, unit) = tile_spec(k).ok_or_else(|| {
            format!(
                "sections[{i}] Details: unknown tile {k:?} — permitted: \
                 aqi, uv, humidity, wind, pressure"
            )
        })?;
        let cap = if zh { cap_zh } else { cap_en };
        let call = if helper == "airquality" {
            format!("sys.airquality({ll}, {path:?})")
        } else {
            format!("sys.weather({ll}, {path:?})")
        };
        let value = if unit.is_empty() { call } else { format!("{call} + {unit:?}") };
        cells.push(format!(
            "\x20               RoundedView{{ width: Fill height: Fit flow: Down \
             draw_bg.color: {tile} draw_bg.border_radius: {tr} \
             padding: Inset{{left: 14 top: 12 right: 14 bottom: 12}} spacing: 6\n\
             \x20                   TextCaption{{ text: {cap:?} draw_text.color: {faint} }}\n\
             \x20                   TextValue{{ text: {value} draw_text.color: {text} }}\n\
             \x20               }}",
            tile = theme::TILE,
            tr = theme::TILE_RADIUS,
            faint = theme::TEXT_FAINT,
            text = theme::TEXT,
        ));
    }
    let mut rows = String::new();
    for pair in cells.chunks(2) {
        rows.push_str(&format!(
            "\x20           View{{ width: Fill height: Fit flow: Right spacing: 10 margin: Inset{{top: 10}}\n{}\n\x20           }}\n",
            pair.join("\n")
        ));
    }
    Ok(format!(
        "\x20       View{{ width: Fill height: Fit flow: Down margin: Inset{{top: {gap}}}\n{rows}\x20       }}",
        gap = theme::GAP,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KYOTO: &str = r#"{
        "plan": "weather", "locale": "en",
        "place": { "query": "Kyoto" },
        "photo": "kyoto city cloudy sky",
        "sections": [
            { "block": "CurrentConditions", "args": { "condition": "cloudy" } },
            { "block": "Forecast", "args": { "days": 7,
              "conditions": ["cloudy","partly_cloudy","rain","cloudy","clear","partly_cloudy","cloudy"] } },
            { "block": "AirQualityField" },
            { "block": "SunMoon" },
            { "block": "Details", "args": { "tiles": ["aqi","uv","humidity","wind"] } }
        ]
    }"#;

    /// The invariants that were each a live bug. Every one is now a property of the
    /// lowering rather than a rule the model is asked to follow.
    #[test]
    fn lowered_card_holds_every_invariant() {
        let out = lower_plan(KYOTO).expect("valid plan must lower");

        // Exactly one top-level node — a sibling would take half the screen width.
        assert_eq!(
            out.lines().filter(|l| l.starts_with("SolidView{")).count(),
            1,
            "exactly one top-level node"
        );
        // Coordinates are resolved, never typed.
        assert!(out.contains("sys.geocodenum(\"Kyoto\", \"lat\")"));
        assert!(!out.contains("35.0"), "no literal coordinate may appear");
        // The week's extent is derived, never guessed.
        assert!(out.contains("sys.weekmin("));
        assert!(!out.contains("draw_bg.wmin: 1"), "no literal week extent");
        // Weekdays come from the forecast's own dates.
        assert_eq!(out.matches("sys.dayname(").count(), 7);
        // Typography is a role; no font file or chain can appear.
        assert!(out.contains("TextHero{"));
        assert!(!out.contains("font_family"));
        assert!(!out.contains("crate_resource"));
        // The AQI field is fetched by the widget, not enumerated in the card.
        assert!(!out.contains("draw_bg.a0"));
        // Root is Fit and the photo outruns the content.
        assert!(out.contains("height: Fit flow: Overlay"));
        assert!(out.contains("height: 2000"));
        assert!(!out.contains("height: 858"));
    }

    #[test]
    fn a_coordinate_cannot_even_be_expressed() {
        let bad = r#"{"plan":"weather","locale":"en",
            "place":{"query":"Kyoto","lat":35.0},"sections":[]}"#;
        let err = lower_plan(bad).unwrap_err();
        assert!(err.contains("lat"), "unknown field must be named: {err}");
    }

    #[test]
    fn unknown_block_is_rejected_with_the_permitted_set() {
        let bad = r#"{"plan":"weather","locale":"en","place":{"query":"Kyoto"},
            "sections":[{"block":"PollenChart"}]}"#;
        let err = lower_plan(bad).unwrap_err();
        assert!(err.contains("PollenChart") && err.contains("CurrentConditions"), "{err}");
    }

    #[test]
    fn unknown_condition_is_rejected_rather_than_drawn_as_clear_sky() {
        let bad = r#"{"plan":"weather","locale":"en","place":{"query":"Kyoto"},
            "sections":[{"block":"CurrentConditions","args":{"condition":"drizzly"}}]}"#;
        let err = lower_plan(bad).unwrap_err();
        assert!(err.contains("drizzly") && err.contains("partly_cloudy"), "{err}");
    }

    /// A Chinese plan must produce a card with no English label, and must select the
    /// Chinese moon-phase names.
    #[test]
    fn locale_drives_every_label() {
        let zh = KYOTO.replace(r#""locale": "en""#, r#""locale": "zh""#);
        let out = lower_plan(&zh).expect("zh plan must lower");
        assert!(out.contains("多云"));
        assert!(out.contains("空气质量图"));
        assert!(out.contains("name_zh"));
        assert!(!out.contains("Air Quality"));
        assert!(!out.contains("\"Cloudy\""));
    }

    /// The plan is far smaller than the card it produces — that ratio IS the
    /// reduction in error surface.
    #[test]
    fn plan_is_far_smaller_than_the_card() {
        let out = lower_plan(KYOTO).unwrap();
        assert!(
            out.len() > KYOTO.len() * 8,
            "card {} vs plan {}",
            out.len(),
            KYOTO.len()
        );
    }
}
