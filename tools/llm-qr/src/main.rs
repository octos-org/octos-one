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
//! cargo run --manifest-path tools/llm-qr/Cargo.toml -- --family zai --model glm-5.2 --prompt-key --svg /tmp/zai.svg
//! cargo run --manifest-path tools/llm-qr/Cargo.toml -- --json '{"llm_family":"zai",...}'
//! ```
//!
//! By default it prints a Unicode QR to the terminal. `--svg` is preferred for
//! camera scanning because terminal font metrics can distort QR geometry.
//! NOTE: the generated QR contains the key — treat it like a password.

use qrcode::render::{svg, unicode};
use qrcode::{EcLevel, QrCode};
use serde_json::{Map, Value};
use std::path::Path;
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
    prompt_key: bool,
    json: Option<String>,
    svg: Option<String>,
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
        prompt_key: false,
        json: None,
        svg: None,
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
            "--prompt-key" => a.prompt_key = true,
            "--json" => a.json = val(),
            "--svg" => a.svg = val(),
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
         --prompt-key      securely prompt for the API key without shell history\n  \
         --json   <json>   encode an LLM-only JSON payload instead\n  \
         --svg    <path>   also write a large, high-contrast SVG QR image\n  \
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

/// Render a Unicode QR, or a high-contrast SVG when a path is provided.
fn render_qr(payload: &str, svg_path: Option<&str>) {
    let code =
        QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M).unwrap_or_else(|e| {
            die(&format!(
                "error: could not encode QR (payload too long?): {e}"
            ))
        });
    if let Some(path) = svg_path {
        let image = code
            .render::<svg::Color>()
            .min_dimensions(1024, 1024)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .quiet_zone(true)
            .build();
        std::fs::write(Path::new(path), image)
            .unwrap_or_else(|e| die(&format!("error: could not write SVG {path:?}: {e}")));
        println!("Wrote scannable QR image: {path}");
    } else {
        // Dense1x2 packs two module rows per line; swapping the colors inverts
        // it for a typical dark terminal. Some fonts distort this rendering,
        // which is why camera provisioning should prefer `--svg`.
        let art = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .quiet_zone(true)
            .build();
        println!("{art}");
    }
}

fn main() {
    let mut a = parse_args();
    if a.prompt_key {
        if a.key.is_some() || a.json.is_some() {
            die("error: --prompt-key cannot be combined with --key or --json");
        }
        a.key = Some(
            rpassword::prompt_password("API key: ")
                .unwrap_or_else(|e| die(&format!("error: could not read API key: {e}"))),
        );
    }
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
    println!("QR payload generated (contains your API key; value not printed).\n");
    render_qr(&payload, a.svg.as_deref());
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
            prompt_key: false,
            json: None,
            svg: None,
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
            prompt_key: false,
            json: Some(
                r#"{"llm_family":"zai","llm_key":"sk-test","base_url":"https://example.com"}"#
                    .into(),
            ),
            svg: None,
        });
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[test]
    fn generated_qr_round_trips_through_android_decoder() {
        let payload = build_payload(&Args {
            family: Some("zai".into()),
            model: Some("glm-5.2".into()),
            key: Some("sk-fake-roundtrip-only".into()),
            prompt_key: false,
            json: None,
            svg: None,
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
