//! The `sys.*` host surface — the real porting cost of a second backend.
//!
//! This is the finding the whole investigation converged on, made concrete: moving a
//! Splash card to a new renderer costs DATA HELPERS, not widgets. octos-one exposes
//! about thirty `sys.*` functions that a card calls at render time; a backend without
//! them renders a card full of em dashes, which looks like a network problem and is
//! not.
//!
//! Every helper below mirrors its octos-one twin's semantics deliberately, including
//! the parts that are easy to get subtly wrong:
//!
//! * `geocode` picks its lookup LANGUAGE from the script of the query — open-meteo
//!   indexes per language, so "上海" with `language=en` returns nothing at all.
//! * `weather` rounds temperature, UV and wind to whole numbers, and only those; an AQI
//!   or a humidity reading passes through untouched.
//! * `dayname` reads the weekday from the FORECAST'S own dates, not this device's
//!   clock, so a Kyoto card seen from California still starts on Kyoto's today.
//! * `weathercond` and `weatherword` both derive from the same live `weather_code`, so
//!   the icon and the words cannot disagree with each other or with the sky.
//! * `weekmin`/`weekmax` exist because a card cannot know them: the values are a live
//!   fetch, so anything that guesses the range clamps every gradient to one end.
//!
//! Responses are cached per exact URL, so ~45 field reads across one card cost two
//! requests.

use splash_core::vm as ms;
use ms::makepad_live_id::*;
use ms::traits::*;
use ms::*;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Named for the on-screen capability report: what a card may call here.
pub const CAPABILITIES: &[&str] = &[
    "sys.geocode",
    "sys.geocodenum",
    "sys.weather",
    "sys.weathernum",
    "sys.weathercond",
    "sys.weatherword",
    "sys.dayname",
    "sys.weekmin",
    "sys.weekmax",
    "sys.airquality",
    "sys.news",
    "sys.movers",
    "sys.stock",
    "sys.stockrange",
    "sys.photo",
];

static CACHE: Mutex<Option<BTreeMap<String, String>>> = Mutex::new(None);
static REQUESTS: AtomicUsize = AtomicUsize::new(0);
static HITS: AtomicUsize = AtomicUsize::new(0);

/// Human-readable cache stats — evidence that N field reads cost one request.
pub fn stats() -> String {
    format!(
        "{} request(s), {} cache hit(s)",
        REQUESTS.load(Ordering::Relaxed),
        HITS.load(Ordering::Relaxed)
    )
}

fn get(url: &str) -> Option<String> {
    if let Some(v) = CACHE.lock().unwrap().as_ref().and_then(|m| m.get(url)).cloned() {
        HITS.fetch_add(1, Ordering::Relaxed);
        return Some(v);
    }
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    let body = ureq::get(url)
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .ok()?
        .into_string()
        .ok()?;
    CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(BTreeMap::new)
        .insert(url.to_string(), body.clone());
    Some(body)
}

fn json(url: &str) -> Option<serde_json::Value> {
    serde_json::from_str(&get(url)?).ok()
}

fn at<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = v;
    for seg in path.split('.') {
        cur = match seg.parse::<usize>() {
            Ok(i) => cur.get(i)?,
            Err(_) => cur.get(seg)?,
        };
    }
    Some(cur)
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn wx_url(lat: f64, lon: f64) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
&current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m\
&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,uv_index_max\
&timezone=auto&forecast_days=7"
    )
}

fn geocode_url(name: &str) -> String {
    let cjk = name.chars().any(|c| {
        matches!(c,
            '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
    });
    let lang = if cjk { "zh" } else { "en" };
    format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language={lang}&format=json",
        enc(name.trim())
    )
}

fn geocode_field(name: &str, field: &str) -> Option<String> {
    let v = json(&geocode_url(name))?;
    let key = match field.trim() {
        "lat" => "results.0.latitude",
        "lon" => "results.0.longitude",
        "name" => "results.0.name",
        "country" => "results.0.country",
        "timezone" => "results.0.timezone",
        _ => return None,
    };
    let x = at(&v, key)?;
    Some(x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string()))
}

/// Whole-number display for the three quantities a card shows as a headline figure.
/// Scoped BY PATH: an AQI or humidity reading must pass through untouched.
fn round_display(path: &str, v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        // open-meteo ISO datetime -> HH:MM, matching sys.weather.
        if s.len() >= 16 && s.as_bytes().get(10) == Some(&b'T') {
            return s[11..16].to_string();
        }
        return s.to_string();
    }
    let Some(n) = v.as_f64() else {
        return "—".to_string();
    };
    if path.contains("temperature") || path.contains("uv_index") || path.contains("wind_speed") {
        format!("{}", n.round() as i64)
    } else {
        format!("{n}")
    }
}

fn wmo_cond(code: Option<i64>) -> f64 {
    match code {
        Some(0) => 0.0,
        Some(1) | Some(2) => 1.0,
        Some(3) => 2.0,
        Some(45) | Some(48) => 7.0,
        Some(51..=57) | Some(61..=67) | Some(80..=82) => 3.0,
        Some(71..=77) | Some(85) | Some(86) => 5.0,
        Some(95..=99) => 4.0,
        _ => 1.0,
    }
}

fn wmo_word(code: Option<i64>, zh: bool) -> &'static str {
    let (en, cn) = match code {
        Some(0) => ("Clear", "晴"),
        Some(1) => ("Mainly Clear", "晴间多云"),
        Some(2) => ("Partly Cloudy", "局部多云"),
        Some(3) => ("Overcast", "阴"),
        Some(45) | Some(48) => ("Fog", "雾"),
        Some(51..=57) => ("Drizzle", "小雨"),
        Some(61..=67) => ("Rain", "雨"),
        Some(71..=77) => ("Snow", "雪"),
        Some(80..=82) => ("Showers", "阵雨"),
        Some(85) | Some(86) => ("Snow Showers", "阵雪"),
        Some(95..=99) => ("Thunderstorm", "雷暴"),
        _ => ("—", "—"),
    };
    if zh {
        cn
    } else {
        en
    }
}

fn days_from_civil(y: i64, m: u64, d: u64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

const DAY_EN: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAY_ZH: [&str; 7] = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

/// Register the `sys.*` module a card calls.
///
/// The names and argument shapes match octos-one's exactly, because the card being
/// evaluated IS octos-one's — read from its saved cards, not transcribed. A mismatch
/// here does not fail loudly; it renders an em dash, which is why the app also shows a
/// capability list.
pub fn register(vm: &mut ScriptVm) {
    let sys = vm.new_module(id!(sys));

    // `ident`, not `expr`: script_value! matches its vm as an ident, and passing an
    // expr metavariable through fails to match its rules.
    macro_rules! sarg {
        ($vm:ident, $args:ident, $f:ident) => {{
            let v = script_value!($vm, $args.$f);
            let mut s = String::new();
            $vm.bx.heap.cast_to_string(v, &mut s);
            s
        }};
    }
    macro_rules! narg {
        ($vm:ident, $args:ident, $f:ident) => {
            script_value!($vm, $args.$f).as_number().unwrap_or(0.0)
        };
    }

    vm.add_method(
        sys,
        id_lut!(geocode),
        script_args_def!(name = NIL, field = NIL),
        |vm, args| {
            let name = sarg!(vm, args, name);
            let field = sarg!(vm, args, field);
            let out = geocode_field(&name, &field).unwrap_or_else(|| "—".into());
            vm.bx.heap.new_string_from_str(&out)
        },
    );

    vm.add_method(
        sys,
        id_lut!(geocodenum),
        script_args_def!(name = NIL, field = NIL),
        |vm, args| {
            let name = sarg!(vm, args, name);
            let field = sarg!(vm, args, field);
            // -9999 is octos-one's loading sentinel; a card treats it as "not ready".
            let n = geocode_field(&name, &field)
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(-9999.0);
            ScriptValue::from_f64(n)
        },
    );

    vm.add_method(
        sys,
        id_lut!(weather),
        script_args_def!(lat = NIL, lon = NIL, path = NIL),
        |vm, args| {
            let (lat, lon) = (narg!(vm, args, lat), narg!(vm, args, lon));
            let path = sarg!(vm, args, path);
            let out = json(&wx_url(lat, lon))
                .as_ref()
                .and_then(|j| at(j, path.trim()).cloned())
                .map(|v| round_display(path.trim(), &v))
                .unwrap_or_else(|| "—".into());
            vm.bx.heap.new_string_from_str(&out)
        },
    );

    vm.add_method(
        sys,
        id_lut!(weathernum),
        script_args_def!(lat = NIL, lon = NIL, path = NIL),
        |vm, args| {
            let (lat, lon) = (narg!(vm, args, lat), narg!(vm, args, lon));
            let path = sarg!(vm, args, path);
            let n = json(&wx_url(lat, lon))
                .as_ref()
                .and_then(|j| at(j, path.trim()).and_then(|v| v.as_f64()))
                .unwrap_or(-9999.0);
            ScriptValue::from_f64(n)
        },
    );

    vm.add_method(
        sys,
        id_lut!(weathercond),
        script_args_def!(lat = NIL, lon = NIL, path = NIL),
        |vm, args| {
            let (lat, lon) = (narg!(vm, args, lat), narg!(vm, args, lon));
            let path = sarg!(vm, args, path);
            let code = json(&wx_url(lat, lon))
                .as_ref()
                .and_then(|j| at(j, path.trim()).and_then(|v| v.as_f64()))
                .map(|n| n as i64);
            ScriptValue::from_f64(wmo_cond(code))
        },
    );

    vm.add_method(
        sys,
        id_lut!(weatherword),
        script_args_def!(lat = NIL, lon = NIL, path = NIL, locale = NIL),
        |vm, args| {
            let (lat, lon) = (narg!(vm, args, lat), narg!(vm, args, lon));
            let path = sarg!(vm, args, path);
            let zh = sarg!(vm, args, locale).trim().starts_with("zh");
            let code = json(&wx_url(lat, lon))
                .as_ref()
                .and_then(|j| at(j, path.trim()).and_then(|v| v.as_f64()))
                .map(|n| n as i64);
            vm.bx.heap.new_string_from_str(wmo_word(code, zh))
        },
    );

    vm.add_method(
        sys,
        id_lut!(dayname),
        script_args_def!(lat = NIL, lon = NIL, n = NIL, locale = NIL),
        |vm, args| {
            let (lat, lon) = (narg!(vm, args, lat), narg!(vm, args, lon));
            let n = narg!(vm, args, n).max(0.0) as usize;
            let zh = sarg!(vm, args, locale).trim().starts_with("zh");
            if n == 0 {
                let s = if zh { "今天" } else { "Today" };
                return vm.bx.heap.new_string_from_str(s);
            }
            // The date comes from the FORECAST, so the label belongs to the place being
            // shown rather than to wherever this phone is.
            let out = json(&wx_url(lat, lon))
                .as_ref()
                .and_then(|j| at(j, &format!("daily.time.{n}")).and_then(|v| v.as_str().map(str::to_string)))
                .and_then(|date| {
                    let mut it = date.split('-');
                    let y = it.next()?.parse::<i64>().ok()?;
                    let m = it.next()?.parse::<u64>().ok()?;
                    let d = it.next()?.parse::<u64>().ok()?;
                    let wd = (((days_from_civil(y, m, d) + 4) % 7 + 7) % 7) as usize;
                    Some(if zh { DAY_ZH[wd] } else { DAY_EN[wd] }.to_string())
                })
                .unwrap_or_else(|| "—".into());
            vm.bx.heap.new_string_from_str(&out)
        },
    );

    fn extent(lat: f64, lon: f64, path: &str, want_max: bool) -> f64 {
        let Some(v) = json(&wx_url(lat, lon)) else {
            return if want_max { 30.0 } else { 0.0 };
        };
        let mut acc: Option<f64> = None;
        for i in 0..7 {
            let Some(n) = at(&v, &format!("{path}.{i}")).and_then(|x| x.as_f64()) else {
                continue;
            };
            acc = Some(match acc {
                None => n,
                Some(a) if want_max => a.max(n),
                Some(a) => a.min(n),
            });
        }
        acc.unwrap_or(if want_max { 30.0 } else { 0.0 })
    }

    vm.add_method(
        sys,
        id_lut!(weekmin),
        script_args_def!(lat = NIL, lon = NIL),
        |vm, args| {
            let (lat, lon) = (narg!(vm, args, lat), narg!(vm, args, lon));
            ScriptValue::from_f64(extent(lat, lon, "daily.temperature_2m_min", false))
        },
    );
    vm.add_method(
        sys,
        id_lut!(weekmax),
        script_args_def!(lat = NIL, lon = NIL),
        |vm, args| {
            let (lat, lon) = (narg!(vm, args, lat), narg!(vm, args, lon));
            ScriptValue::from_f64(extent(lat, lon, "daily.temperature_2m_max", true))
        },
    );

    vm.add_method(
        sys,
        id_lut!(airquality),
        script_args_def!(lat = NIL, lon = NIL, path = NIL),
        |vm, args| {
            let (lat, lon) = (narg!(vm, args, lat), narg!(vm, args, lon));
            let path = sarg!(vm, args, path);
            let url = format!(
                "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={lat:.4}\
&longitude={lon:.4}&current=us_aqi,pm2_5,pm10,ozone&timezone=auto"
            );
            let out = json(&url)
                .as_ref()
                .and_then(|j| at(j, path.trim()).cloned())
                .map(|v| round_display(path.trim(), &v))
                .unwrap_or_else(|| "—".into());
            vm.bx.heap.new_string_from_str(&out)
        },
    );

    vm.add_method(
        sys,
        id_lut!(news),
        script_args_def!(index = NIL, key = NIL),
        |vm, args| {
            let i = narg!(vm, args, index).max(0.0) as usize;
            let key = sarg!(vm, args, key);
            let out = (|| {
                let ids = json("https://hacker-news.firebaseio.com/v0/topstories.json")?;
                let id = ids.get(i)?.as_i64()?;
                let item = json(&format!(
                    "https://hacker-news.firebaseio.com/v0/item/{id}.json"
                ))?;
                let k = match key.trim() {
                    "title" => "title",
                    "author" => "by",
                    "points" => "score",
                    "comments" => "descendants",
                    "url" => "url",
                    _ => return None,
                };
                let v = item.get(k)?;
                Some(
                    v.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string()),
                )
            })()
            .unwrap_or_else(|| "—".into());
            vm.bx.heap.new_string_from_str(&out)
        },
    );


    // ---- market data ------------------------------------------------------
    //
    // Yahoo Finance, the same source octos-one uses. `movers` needs no ticker: who is
    // moving is the market's answer, not the model's.

    fn yahoo_quote(sym: &str) -> Option<serde_json::Value> {
        let url = format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?range=1d&interval=1d",
            enc(sym)
        );
        at(&json(&url)?, "chart.result.0").cloned()
    }

    fn movers_list() -> Option<serde_json::Value> {
        let url = "https://query1.finance.yahoo.com/v1/finance/screener/predefined/saved\
?scrIds=day_gainers&count=10";
        at(&json(url)?, "finance.result.0.quotes").cloned()
    }

    vm.add_method(
        sys,
        id_lut!(movers),
        script_args_def!(index = NIL, field = NIL),
        |vm, args| {
            let i = narg!(vm, args, index).max(0.0) as usize;
            let field = sarg!(vm, args, field);
            let out = movers_list()
                .and_then(|q| q.get(i).cloned())
                .and_then(|row| {
                    let key = match field.trim() {
                        "symbol" => "symbol",
                        "name" => "shortName",
                        "price" => "regularMarketPrice",
                        "change" => "regularMarketChange",
                        "changepct" => "regularMarketChangePercent",
                        _ => return None,
                    };
                    let v = row.get(key)?;
                    Some(match v.as_f64() {
                        // Two decimals for money, and a sign on a percentage — the
                        // direction must be readable without colour.
                        Some(n) if key == "regularMarketChangePercent" => {
                            format!("{}{:.2}%", if n >= 0.0 { "+" } else { "" }, n)
                        }
                        Some(n) => format!("{n:.2}"),
                        None => v.as_str().unwrap_or("—").to_string(),
                    })
                })
                .unwrap_or_else(|| "—".into());
            vm.bx.heap.new_string_from_str(&out)
        },
    );

    vm.add_method(
        sys,
        id_lut!(stock),
        script_args_def!(symbol = NIL, key = NIL),
        |vm, args| {
            let sym = sarg!(vm, args, symbol);
            let key = sarg!(vm, args, key);
            let out = yahoo_quote(&sym)
                .and_then(|r| {
                    let m = at(&r, "meta")?;
                    let path = match key.trim() {
                        "symbol" => "symbol",
                        "name" => "longName",
                        "price" => "regularMarketPrice",
                        "prev" => "previousClose",
                        "high" => "regularMarketDayHigh",
                        "low" => "regularMarketDayLow",
                        "open" => "regularMarketOpen",
                        "currency" => "currency",
                        _ => return None,
                    };
                    let v = m.get(path)?;
                    Some(match v.as_f64() {
                        Some(n) => format!("{n:.2}"),
                        None => v.as_str().unwrap_or("—").to_string(),
                    })
                })
                .unwrap_or_else(|| "—".into());
            vm.bx.heap.new_string_from_str(&out)
        },
    );

    vm.add_method(
        sys,
        id_lut!(stockrange),
        script_args_def!(symbol = NIL, range = NIL, key = NIL),
        |vm, args| {
            let sym = sarg!(vm, args, symbol);
            let _range = sarg!(vm, args, range);
            let key = sarg!(vm, args, key);
            let out = yahoo_quote(&sym)
                .and_then(|r| {
                    let m = at(&r, "meta")?;
                    let price = m.get("regularMarketPrice")?.as_f64()?;
                    let prev = m.get("previousClose")?.as_f64()?;
                    let d = price - prev;
                    Some(match key.trim() {
                        // Direction is FETCHED. A card that asserted it would paint a red
                        // day green for as long as it exists.
                        "up" => (if d >= 0.0 { "1" } else { "0" }).to_string(),
                        "change" => format!("{}{:.2}", if d >= 0.0 { "+" } else { "" }, d),
                        "changepct" => format!(
                            "{}{:.2}%",
                            if d >= 0.0 { "+" } else { "" },
                            if prev != 0.0 { d / prev * 100.0 } else { 0.0 }
                        ),
                        _ => "—".to_string(),
                    })
                })
                .unwrap_or_else(|| "—".into());
            vm.bx.heap.new_string_from_str(&out)
        },
    );

    // A backdrop photo is a URL octos-one resolves through its own image service. There
    // is no equivalent here, so it answers with an empty string — the card then draws
    // no image rather than a broken one.
    vm.add_method(sys, id_lut!(photo), script_args_def!(q = NIL), |vm, _a| {
        vm.bx.heap.new_string_from_str("")
    });

    // Creating the module is not enough: without this, `sys` is not a bare name in the
    // card's scope and every `sys.foo(...)` evaluates to WrongValue — which the walker
    // then renders as the literal text "[Error:WrongValue]" rather than failing. Found
    // by rendering a card where every value came out as that string while the fetch
    // counter stayed at zero. octos-one does the same thing at splash.rs:1602.
    vm.set_injected_global(id!(sys), sys.into());
}
