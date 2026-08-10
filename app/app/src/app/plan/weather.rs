//! Weather plans → Splash DSL.
//!
//! The domain whose failure modes are documented, so it went first. See the module
//! doc on [`super`] for the rule every domain follows.

use super::common::{locale_tag, photo_root, theme};
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
    /// How many forecast rows. The only thing about the forecast the model gets to
    /// choose, because it is a presentation decision rather than a fact.
    #[serde(default)]
    pub days: Option<u32>,
    #[serde(default)]
    pub tiles: Vec<String>,
    /// Attractions to recommend, as NAMES. The one field here no tool can
    /// answer; every row is still proved against live data at render time.
    #[serde(default)]
    pub places: Vec<String>,
}

// There is deliberately NO `condition` field, and no per-day `conditions` array.
//
// The weather condition is LIVE DATA — it comes from `weather_code` in the forecast
// the card fetches at render time. An earlier version of this schema accepted a
// condition WORD from the model, which was the same mistake as accepting a
// coordinate wearing different clothes: a value the model has never observed,
// stated confidently, with nothing able to contradict it. Seven more of them in the
// forecast array.
//
// `sys.weathercond(lat, lon, path)` maps the live code to an icon instead. The code
// was already in the fetch — the model was being asked to guess data the runtime
// held.

/// A condition word → the WeatherIcon shader index and a forecast-row emoji.


/// Attractions the model recommends.
///
/// The names are the model's, and stay the model's: which sights are worth
/// seeing is the one thing here that no tool answers, and a place NAME is
/// already what the plan supplies for `place.query`. What the model may not do
/// is attach anything it has not observed — no distance, no description, no
/// opening time — so a row is a name and nothing else.
///
/// Three live resolvers were tried and all three failed, each for its own
/// reason, and none is worth a fourth attempt:
///   * `sys.geocode` is a gazetteer of POPULATED places — four of five Shanghai
///     landmarks resolved to nothing.
///   * Nominatim answers exactly this question and works from a laptop, but
///     returns 403 to the device for every request even with the identifying UA
///     its policy demands. A 403 is permanent, so every row went terminal.
///   * Overpass by name matches OSM's LOCAL name (外滩, not "The Bund"), and a
///     name regex over a city-sized radius times out (504).
fn attractions(a: &Args, i: usize, zh: bool) -> Result<String, String> {
    if a.places.is_empty() {
        return Err(format!(
            "sections[{i}] Attractions: name at least one place in `places`"
        ));
    }
    let heading = if zh { "景点" } else { "WORTH SEEING" };
    let mut rows = String::new();
    for p in &a.places {
        let q = p.trim();
        if q.is_empty() || q.chars().count() > 64 {
            return Err(format!(
                "sections[{i}] Attractions: {p:?} is not a place name"
            ));
        }
        rows.push_str(&format!(
            "\x20           TextRow{{ width: Fill text: {q:?} draw_text.color: {text} \
             margin: Inset{{top: 8 bottom: 8}} }}\n",
            text = theme::TEXT,
        ));
    }
    Ok(format!(
        "\x20       View{{ width: Fill height: Fit flow: Down \
         padding: Inset{{left: 20 top: 18 right: 20 bottom: 8}}\n\
         \x20           TextCaption{{ text: {heading:?} draw_text.color: {faint} }}\n\
         {rows}\x20       }}\n",
        faint = theme::TEXT_FAINT,
    ))
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

/// Lower a plan to Splash DSL, or explain why it cannot be lowered.
///
/// The error strings are written for the repair loop: they name the offending
/// field and the permitted values, so a retry is targeted rather than a whole-card
/// regeneration.
pub fn lower(json: &str) -> Result<String, String> {
    let plan: Plan = serde_json::from_str(json)
        .map_err(|e| format!("weather plan is not valid: {e}"))?;
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
    let loc = locale_tag(&plan.locale);
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
            "CurrentConditions" => current_conditions(&ll, place, loc),
            "Forecast" => forecast(&sec.args, &ll, loc),
            "AirQualityField" => air_quality_field(&ll, &lat, &lon, zh),
            "SunMoon" => sun_moon(&ll, zh),
            "Details" => details(&sec.args, &ll, zh, i)?,
            "Attractions" => attractions(&sec.args, i, zh)?,
            other => {
                return Err(format!(
                    "sections[{i}]: unknown block {other:?} — permitted: \
                     CurrentConditions, Forecast, AirQualityField, SunMoon, Details, Attractions"
                ))
            }
        };
        body.push_str(&s);
        body.push('\n');
    }

    let photo = if plan.photo.trim().is_empty() {
        place.clone()
    } else {
        plan.photo.clone()
    };
    Ok(photo_root("weather-app", &photo, &body))
}

fn current_conditions(ll: &str, place: &str, loc: &str) -> String {
    format!(
        "\x20       View{{ width: Fill height: Fit flow: Down align: Align{{x: 0.5}}\n\
         \x20           TextTitle{{ text: sys.geocode({place:?}, \"name\") draw_text.color: {soft} }}\n\
         \x20           TextHero{{ text: sys.weather({ll}, \"current.temperature_2m\") + \"°\" \
         margin: Inset{{top: 2 bottom: 0}} draw_text.color: {text} }}\n\
         \x20           View{{ width: Fit height: 52 flow: Right align: Align{{x: 0.5 y: 0.5}} spacing: 8\n\
         \x20               WeatherIcon{{ draw_bg.cond: sys.weathercond({ll}, \"current.weather_code\") \
         width: 46 height: 46 }}\n\
         \x20               TextBody{{ text: sys.weatherword({ll}, \"current.weather_code\", {loc:?}) \
         draw_text.color: {soft} }}\n\
         \x20           }}\n\
         \x20           TextStat{{ text: \"↑\" + sys.weather({ll}, \"daily.temperature_2m_max.0\") + \"°   ↓\" \
         + sys.weather({ll}, \"daily.temperature_2m_min.0\") + \"°   ≈\" \
         + sys.weather({ll}, \"current.apparent_temperature\") + \"°\" draw_text.color: {dim} }}\n\
         \x20       }}",
        soft = theme::TEXT_SOFT,
        text = theme::TEXT,
        dim = theme::TEXT_DIM,
        loc = loc,
    )
}

fn forecast(a: &Args, ll: &str, loc: &str) -> String {
    let days = a.days.unwrap_or(7).clamp(1, 7) as usize;
    let mut rows = String::new();
    for d in 0..days {
        rows.push_str(&format!(
            "\x20           View{{ width: Fill height: {rh} flow: Right align: Align{{y: 0.5}}\n\
             \x20               TextRow{{ width: {dw} text: sys.dayname({ll}, {d}, {loc:?}) draw_text.color: {soft} }}\n\
             \x20               WeatherIcon{{ width: {iw} height: 26 \
             draw_bg.cond: sys.weathercond({ll}, \"daily.weather_code.{d}\") }}\n\
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
    format!(
        "\x20       RoundedView{{ width: Fill height: Fit flow: Down new_batch: true \
         draw_bg.color: {panel} draw_bg.border_radius: {pr} margin: Inset{{top: {gap}}} \
         padding: Inset{{left: 16 right: 16 top: 10 bottom: 10}}\n{rows}\x20       }}",
        panel = theme::PHOTO_PANEL,
        pr = theme::PANEL_RADIUS,
        gap = theme::GAP,
    )
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
        panel = theme::PHOTO_PANEL,
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
            tile = theme::PHOTO_TILE,
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
            { "block": "CurrentConditions" },
            { "block": "Forecast", "args": { "days": 7 } },
            { "block": "AirQualityField" },
            { "block": "SunMoon" },
            { "block": "Details", "args": { "tiles": ["aqi","uv","humidity","wind"] } }
        ]
    }"#;

    /// A live query authored against this schema, not the prototype's: no
    /// `schema`/`theme` keys, and no `condition` word anywhere.
    const SHANGHAI_LIVE: &str = r#"{
        "plan": "weather", "locale": "en",
        "place": { "query": "Shanghai" },
        "photo": "shanghai bund skyline summer haze",
        "sections": [
            { "block": "CurrentConditions" },
            { "block": "Forecast", "args": { "days": 7 } },
            { "block": "AirQualityField" },
            { "block": "SunMoon" },
            { "block": "Details", "args": { "tiles": ["aqi","uv","humidity","wind"] } }
        ]
    }"#;

    /// "is it a good time to go to shanghai now" — weather AND the attractions
    /// that query asks for, which had no block at all.
    #[test]
    fn shanghai_with_attractions_lowers() {
        let plan = r#"{
            "plan": "weather", "locale": "en",
            "place": { "query": "Shanghai" },
            "photo": "shanghai bund skyline summer",
            "sections": [
                { "block": "CurrentConditions" },
                { "block": "Forecast", "args": { "days": 7 } },
                { "block": "Details", "args": { "tiles": ["aqi","uv","humidity","wind"] } },
                { "block": "Attractions", "args": { "places":
                    ["The Bund","Yu Garden","Tianzifang","Longhua Temple"] } }
            ]
        }"#;
        let out = lower(plan).expect("Shanghai + attractions must lower");
        assert!(out.contains("WORTH SEEING"), "the section renders:\n{out}");
        // The names appear; nothing else about them is asserted.
        for p in ["The Bund", "Yu Garden", "Tianzifang"] {
            assert!(out.contains(&format!("text: {p:?}")), "{p} missing:\n{out}");
        }
        assert!(!out.contains("sys.poi("), "no per-row resolver:\n{out}");
        assert!(!out.contains("31.22"), "still no coordinate anywhere:\n{out}");
        eprintln!("SHANGHAI+ATTRACTIONS BYTES = {}", out.len());
        let i = out.find("WORTH SEEING").unwrap();
        eprintln!("--- ATTRACTIONS FRAGMENT ---\n{}\n--- END ---", &out[i.saturating_sub(220)..(i + 700).min(out.len())]);
    }

    /// An empty list is a rejection, not an empty heading.
    #[test]
    fn attractions_needs_at_least_one_place() {
        let plan = r#"{
            "plan": "weather", "locale": "en", "place": { "query": "Shanghai" },
            "photo": "x", "sections": [ { "block": "Attractions", "args": {} } ]
        }"#;
        assert!(lower(plan).unwrap_err().contains("name at least one place"));
    }

    #[test]
    fn shanghai_live_query_lowers() {
        let out = lower(SHANGHAI_LIVE).expect("Shanghai plan must lower");
        assert!(out.contains("Shanghai"), "the place survives");
        assert!(!out.contains("31.22"), "no coordinate may appear in the card:\n{out}");
        eprintln!("SHANGHAI CARD BYTES = {}", out.len());
    }

    /// The prototype's plan shape is rejected here -- it carries `schema` and
    /// `theme`, and states a `condition` the model never observed.
    #[test]
    fn prototype_shaped_plan_is_rejected() {
        let proto = r#"{
            "schema": "octos.card.plan/1", "plan": "weather",
            "theme": "octos.weather.immersive", "locale": "en",
            "place": { "query": "Shanghai" }, "photo": "x",
            "sections": [ { "block": "CurrentConditions", "args": { "condition": "partly_cloudy" } } ]
        }"#;
        let err = lower(proto).expect_err("must reject unknown fields");
        eprintln!("REJECTED AS = {err}");
    }

    /// `Attractions` is a real block now; an unknown one still names the set.
    #[test]
    fn the_permitted_set_now_includes_attractions() {
        let p = r#"{
            "plan": "weather", "locale": "en",
            "place": { "query": "Shanghai" }, "photo": "x",
            "sections": [ { "block": "Nightlife", "args": {} } ]
        }"#;
        let err = lower(p).expect_err("unknown block");
        assert!(err.contains("Attractions"), "the set must advertise it: {err}");
    }

    /// The invariants that were each a live bug. Every one is now a property of the
    /// lowering rather than a rule the model is asked to follow.
    #[test]
    fn lowered_card_holds_every_invariant() {
        let out = lower(KYOTO).expect("valid plan must lower");

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
        let err = lower(bad).unwrap_err();
        assert!(err.contains("lat"), "unknown field must be named: {err}");
    }

    #[test]
    fn unknown_block_is_rejected_with_the_permitted_set() {
        let bad = r#"{"plan":"weather","locale":"en","place":{"query":"Kyoto"},
            "sections":[{"block":"PollenChart"}]}"#;
        let err = lower(bad).unwrap_err();
        assert!(err.contains("PollenChart") && err.contains("CurrentConditions"), "{err}");
    }

    /// The condition is LIVE DATA, so the plan has no field for it at all — the
    /// same rule as a coordinate. A plan that tries to state the weather is
    /// rejected, not merely ignored.
    #[test]
    fn the_weather_itself_cannot_be_stated_by_the_model() {
        let bad = r#"{"plan":"weather","locale":"en","place":{"query":"Kyoto"},
            "sections":[{"block":"CurrentConditions","args":{"condition":"cloudy"}}]}"#;
        let err = lower(bad).unwrap_err();
        assert!(err.contains("condition"), "unknown field must be named: {err}");
    }

    /// Icon AND word both come from the same live weather code, so they cannot
    /// disagree with each other or with the sky.
    #[test]
    fn condition_is_derived_from_the_live_code() {
        let out = lower(KYOTO).unwrap();
        assert!(out.contains("sys.weathercond("), "icon from the live code");
        assert!(out.contains("sys.weatherword("), "word from the live code");
        assert!(!out.contains("draw_bg.cond: 2"), "no literal icon index");
        // Seven forecast rows, each deriving its own day's icon.
        assert_eq!(out.matches("daily.weather_code.").count(), 7);
        // The emoji column is gone with the guesses that fed it.
        for e in ["☀️", "⛅", "☁️", "🌧️"] {
            assert!(!out.contains(e), "no guessed emoji {e}");
        }
    }

    /// A Chinese plan must produce a card with no English label, and must select the
    /// Chinese moon-phase names.
    #[test]
    fn locale_drives_every_label() {
        let zh = KYOTO.replace(r#""locale": "en""#, r#""locale": "zh""#);
        let out = lower(&zh).expect("zh plan must lower");
        assert!(out.contains("\"zh\""), "locale is threaded to the helpers");
        assert!(out.contains("空气质量图"));
        assert!(out.contains("name_zh"));
        assert!(!out.contains("Air Quality"));
        assert!(!out.contains("\"en\""), "no en helper call on a zh card");
    }

    /// The plan is far smaller than the card it produces — that ratio IS the
    /// reduction in error surface.
    #[test]
    fn plan_is_far_smaller_than_the_card() {
        let out = lower(KYOTO).unwrap();
        assert!(
            out.len() > KYOTO.len() * 8,
            "card {} vs plan {}",
            out.len(),
            KYOTO.len()
        );
    }
}
