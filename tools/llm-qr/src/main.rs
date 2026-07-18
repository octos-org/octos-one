//! Encode an octos LLM config into a QR code that the
//! app's composer scans to provision itself — so a user brings their own key
//! without it ever touching the repo, a keyboard, or the network.
//!
//! The QR payload is a single compact JSON object carrying ALL the info (no URL):
//!
//! ```text
//! {"llm_family":"zai","llm_model":"glm-5.2","llm_key":"sk-XXXX"}
//! ```
//!
//! The app parses it and writes `llm_family`/`llm_model` into the octos profile
//! config (`_main.json` → config.llm) and `llm_key` into
//! config.env_vars.<PROVIDER>_API_KEY. Server connection/auth configuration is
//! deliberately not part of this QR format.
//!
//! Usage:
//! ```text
//! cargo run --manifest-path tools/llm-qr/Cargo.toml -- --family zai --model glm-5.2 --key sk-XXXX
//! cargo run --manifest-path tools/llm-qr/Cargo.toml -- --json '{"llm_family":"zai",...}'
//! ```
//!
//! By default it prints the JSON payload and a Unicode QR to the terminal (scan it
//! straight off the screen). NOTE: the key is a secret — treat the QR like a password.

use qrcode::render::unicode;
use qrcode::{EcLevel, QrCode};
use serde_json::{Map, Value};
use std::process::exit;

/// Provider family_id -> the env-var name octos reads the key from. Extend as the
/// octos provider registry grows (crates/octos-llm/src/registry/*). Used only for
/// the "unknown family" hint; the app does the real mapping.
const KNOWN_FAMILIES: &[&str] = &[
    "zai",
    "deepseek",
    "openai",
    "anthropic",
    "gemini",
    "openrouter",
];

struct Args {
    family: Option<String>,
    model: Option<String>,
    key: Option<String>,
    json: Option<String>,
}

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    exit(1);
}

fn parse_args() -> Args {
    let mut a = Args {
        family: None,
        model: None,
        key: None,
        json: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        // Accept `--flag value` and `--flag=value`.
        let (flag, inline) = match flag.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (flag, None),
        };
        let mut val = || inline.clone().or_else(|| it.next());
        match flag.as_str() {
            "--family" => a.family = val(),
            "--model" => a.model = val(),
            "--key" => a.key = val(),
            "--json" => a.json = val(),
            "-h" | "--help" => {
                print_help();
                exit(0);
            }
            other => die(&format!("error: unknown argument '{other}' (try --help)")),
        }
    }
    a
}

fn print_help() {
    println!(
        "Encode an octos LLM config as a QR code (JSON payload).\n\n\
         Options:\n  \
         --family <id>     provider family_id (zai, deepseek, openai, anthropic, …)\n  \
         --model  <id>     model_id (e.g. glm-5.2, deepseek-v4-pro)\n  \
         --key    <key>    the provider API key (stays on-device once scanned)\n  \
         --json   <json>   encode an LLM-only JSON payload instead\n  \
         -h, --help        show this help"
    );
}

/// Build the compact JSON payload (no spaces — QR capacity is limited).
fn build_payload(a: &Args) -> Result<String, String> {
    if let Some(json) = &a.json {
        let v: Value =
            serde_json::from_str(json).map_err(|e| format!("--json is not valid JSON: {e}"))?;
        validate_llm_payload(&v)?;
        return serde_json::to_string(&v).map_err(|e| format!("serialize payload: {e}"));
    }
    let (Some(family), Some(key)) = (&a.family, &a.key) else {
        return Err("--family and --key are required (or pass --json)".into());
    };
    let mut m = Map::new();
    m.insert("llm_family".into(), Value::String(family.clone()));
    m.insert("llm_key".into(), Value::String(key.clone()));
    if let Some(model) = &a.model {
        m.insert("llm_model".into(), Value::String(model.clone()));
    }
    let payload = Value::Object(m);
    validate_llm_payload(&payload)?;
    serde_json::to_string(&payload).map_err(|e| format!("serialize payload: {e}"))
}

fn validate_llm_payload(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "LLM payload must be a JSON object".to_string())?;
    for field in object.keys() {
        if !matches!(field.as_str(), "llm_family" | "llm_model" | "llm_key") {
            return Err(format!(
                "field '{field}' is not allowed in an LLM QR; server configuration uses makepad.APP_CONFIG"
            ));
        }
    }
    for required in ["llm_family", "llm_key"] {
        match object.get(required).and_then(Value::as_str) {
            Some(value) if !value.trim().is_empty() => {}
            _ => return Err(format!("field '{required}' must be a non-empty string")),
        }
    }
    if let Some(model) = object.get("llm_model") {
        if model.as_str().is_none_or(|value| value.trim().is_empty()) {
            return Err("field 'llm_model' must be a non-empty string".into());
        }
    }
    Ok(())
}

/// Render a Unicode QR (half-block chars) that scans straight off a dark terminal.
fn render_qr(payload: &str) {
    let code =
        QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M).unwrap_or_else(|e| {
            die(&format!(
                "error: could not encode QR (payload too long?): {e}"
            ))
        });
    // Dense1x2 packs two module rows per line; swapping the colors inverts it so
    // the dark background of a typical terminal becomes the QR's light field.
    let art = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .quiet_zone(true)
        .build();
    println!("{art}");
}

fn main() {
    let a = parse_args();
    let payload = build_payload(&a).unwrap_or_else(|e| die(&format!("error: {e}")));
    if let Some(family) = &a.family {
        if !KNOWN_FAMILIES.contains(&family.as_str()) {
            eprintln!(
                "note: unknown family '{family}' — the app maps it via octos's \
                 provider registry (key env may fall back to {}_API_KEY).",
                family.to_uppercase()
            );
        }
    }
    println!("QR payload (treat as a secret):\n  {payload}\n");
    render_qr(&payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrcode::types::Color;

    #[test]
    fn generated_payload_contains_only_llm_data() {
        let payload = build_payload(&Args {
            family: Some("zai".into()),
            model: Some("glm-5.2".into()),
            key: Some("sk-test".into()),
            json: None,
        })
        .unwrap();
        let value: Value = serde_json::from_str(&payload).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 3);
        assert!(object.contains_key("llm_family"));
        assert!(object.contains_key("llm_model"));
        assert!(object.contains_key("llm_key"));
    }

    #[test]
    fn json_payload_rejects_server_url() {
        let result = build_payload(&Args {
            family: None,
            model: None,
            key: None,
            json: Some(
                r#"{"llm_family":"zai","llm_key":"sk-test","base_url":"https://example.com"}"#
                    .into(),
            ),
        });
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[test]
    fn generated_qr_round_trips_through_android_decoder() {
        let payload = build_payload(&Args {
            family: Some("zai".into()),
            model: Some("glm-5.2".into()),
            key: Some("sk-fake-roundtrip-only".into()),
            json: None,
        })
        .unwrap();
        let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M).unwrap();

        // Rasterize exactly what the terminal renderer represents: dark modules
        // on a light field, with the standard four-module quiet zone. Scaling
        // gives rqrr a camera-like greyscale frame rather than direct module data.
        let modules = code.width();
        let quiet = 4usize;
        let scale = 6usize;
        let side = (modules + quiet * 2) * scale;
        let mut luma = vec![255u8; side * side];
        for my in 0..modules {
            for mx in 0..modules {
                if code[(mx, my)] == Color::Dark {
                    let x0 = (mx + quiet) * scale;
                    let y0 = (my + quiet) * scale;
                    for y in y0..y0 + scale {
                        luma[y * side + x0..y * side + x0 + scale].fill(0);
                    }
                }
            }
        }

        let mut image =
            rqrr::PreparedImage::prepare_from_greyscale(side, side, |x, y| luma[y * side + x]);
        let decoded = image
            .detect_grids()
            .into_iter()
            .find_map(|grid| grid.decode().ok().map(|(_, content)| content))
            .expect("Android-compatible decoder should detect generated QR");
        assert_eq!(decoded, payload);
    }
}
