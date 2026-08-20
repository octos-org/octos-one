//! Localhost debug monitor — the phone's own progress/console page.
//!
//! One `std::net::TcpListener` thread bound to `127.0.0.1:8686`, reached from
//! a host machine via `adb forward tcp:8686 tcp:8686`. Deliberately NOT a
//! framework server: every route is hand-parsed HTTP/1.1 so the surface stays
//! auditable and dependency-free. Nothing here touches the makepad UI thread —
//! the whole point is that this keeps answering when the UI is the thing that
//! wedged.
//!
//! Routes:
//!   GET  /             the monitor page (inline HTML, no external assets)
//!   GET  /status.json  uptime + every `kv()` the app has published
//!   GET  /events       SSE tail of the `log()` ring (Last-Event-ID honored)
//!   POST /shell        {"cmd": "..."} → /system/bin/sh -c, capped + timed out
//!   POST /imagegen     {"prompt","size","quality"} → OpenAI images API from
//!                      THIS process (the phone dials out, not the host);
//!                      key read from /data/local/tmp/oai_key or OPENAI_API_KEY
//!   GET  /file?path=   serve a file from the allowlisted roots only
//!   GET  /gallery.json newest images/videos under the artifact roots
//!
//! Security posture: loopback bind + adb is the auth boundary — anyone who can
//! reach the port already has full `adb shell`. The file allowlist exists so a
//! stray URL can't read app-private storage, not to stop the adb owner.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};

static LOGS: Mutex<Option<VecDeque<(u64, String)>>> = Mutex::new(None);
static KV: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static SEQ: Mutex<u64> = Mutex::new(0);
static STARTED: OnceLock<std::time::Instant> = OnceLock::new();
static ROOTS: OnceLock<Vec<std::path::PathBuf>> = OnceLock::new();
static OUT_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
static LIB_DIR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
static GEN_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

const LOG_CAP: usize = 2000;
const SHELL_OUT_CAP: usize = 256 * 1024;
const SHELL_TIMEOUT_S: u64 = 120;

/// Append a line to the monitor's ring. Callable from any thread.
pub fn log(line: &str) {
    let mut seq = SEQ.lock().unwrap();
    *seq += 1;
    let id = *seq;
    drop(seq);
    let mut logs = LOGS.lock().unwrap();
    let q = logs.get_or_insert_with(VecDeque::new);
    if q.len() >= LOG_CAP {
        q.pop_front();
    }
    let t = STARTED.get().map(|s| s.elapsed().as_secs()).unwrap_or(0);
    q.push_back((id, format!("[{t:>5}s] {line}")));
}

/// Publish a key into /status.json. Last write wins.
pub fn kv(key: &str, val: impl ToString) {
    let mut kv = KV.lock().unwrap();
    kv.get_or_insert_with(HashMap::new)
        .insert(key.to_string(), val.to_string());
}

/// Start the monitor. `out_dir` is where /imagegen writes artifacts (created
/// here); `extra_root` (e.g. the nativeLibraryDir) joins the file allowlist.
pub fn start(out_dir: std::path::PathBuf, extra_root: Option<std::path::PathBuf>) {
    let _ = STARTED.set(std::time::Instant::now());
    let _ = std::fs::create_dir_all(&out_dir);
    let mut roots = vec![out_dir.clone(), std::path::PathBuf::from("/data/local/tmp")];
    if let Some(r) = extra_root.clone() {
        roots.push(r);
    }
    let _ = ROOTS.set(roots);
    let _ = OUT_DIR.set(out_dir);
    let _ = LIB_DIR.set(extra_root);
    std::thread::spawn(|| {
        // 0.0.0.0 binds every interface incl. wlan0, so the page is reachable
        // over the LAN at http://<phone-wifi-ip>:8686 — not just loopback+adb.
        // SECURITY: this exposes /shell (app-context command exec), /file, and
        // /imagegen to anyone on the same network. Only acceptable on a trusted
        // LAN. Revert to "127.0.0.1:8686" for loopback-only + adb forward.
        let listener = match TcpListener::bind("0.0.0.0:8686") {
            Ok(l) => l,
            Err(e) => {
                log(&format!("monitor: bind failed: {e}"));
                return;
            }
        };
        log("monitor: listening on 0.0.0.0:8686 (LAN-reachable)");
        for stream in listener.incoming() {
            if let Ok(s) = stream {
                std::thread::spawn(move || {
                    let _ = handle(s);
                });
            }
        }
    });
}

fn handle(stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut content_len = 0usize;
    let mut last_event_id = 0u64;
    loop {
        let mut h = String::new();
        reader.read_line(&mut h)?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        let lower = h.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = lower.strip_prefix("last-event-id:") {
            last_event_id = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_len.min(1024 * 1024)];
    if content_len > 0 {
        reader.read_exact(&mut body)?;
    }
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target.as_str(), ""),
    };
    match (method.as_str(), path) {
        ("GET", "/") => respond(stream, 200, "text/html; charset=utf-8", PAGE.as_bytes()),
        ("GET", "/status.json") => status_json(stream),
        ("GET", "/events") => sse(stream, last_event_id),
        ("POST", "/shell") => shell(stream, &body),
        ("POST", "/ffmpeg") => ffmpeg(stream, &body),
        ("POST", "/imagegen") => imagegen(stream, &body),
        ("POST", "/score") => score(stream, &body),
        ("POST", "/concept") => concept(stream, &body),
        ("POST", "/snapshot") => snapshot(stream),
        ("GET", "/file") => file(stream, query),
        ("GET", "/gallery.json") => gallery(stream),
        _ => respond(stream, 404, "text/plain", b"not found"),
    }
}

fn respond(mut s: TcpStream, code: u16, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = if code == 200 { "OK" } else { "ERR" };
    write!(
        s,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    s.write_all(body)
}

fn json_escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 8);
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn status_json(stream: TcpStream) -> std::io::Result<()> {
    let up = STARTED.get().map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let mut body = format!("{{\"uptime_s\":{up},\"pid\":{}", std::process::id());
    if let Some(kv) = KV.lock().unwrap().as_ref() {
        let mut keys: Vec<_> = kv.keys().collect();
        keys.sort();
        for k in keys {
            body.push_str(&format!(
                ",\"{}\":\"{}\"",
                json_escape(k),
                json_escape(&kv[k])
            ));
        }
    }
    body.push('}');
    respond(stream, 200, "application/json", body.as_bytes())
}

fn sse(mut s: TcpStream, mut last_id: u64) -> std::io::Result<()> {
    write!(
        s,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\nretry: 1500\n\n"
    )?;
    // If the client is new, start 200 lines back rather than from the epoch.
    if last_id == 0 {
        if let Some(q) = LOGS.lock().unwrap().as_ref() {
            if q.len() > 200 {
                last_id = q[q.len() - 200].0;
            }
        }
    }
    loop {
        let batch: Vec<(u64, String)> = {
            let logs = LOGS.lock().unwrap();
            logs.as_ref()
                .map(|q| q.iter().filter(|(id, _)| *id > last_id).cloned().collect())
                .unwrap_or_default()
        };
        for (id, line) in batch {
            write!(s, "id: {id}\ndata: {line}\n\n")?;
            last_id = id;
        }
        s.flush()?;
        std::thread::sleep(std::time::Duration::from_millis(400));
        // Heartbeat comment keeps dead-connection detection prompt.
        write!(s, ": hb\n\n")?;
    }
}

/// Minimal JSON string-field extractor for our tiny fixed bodies — avoids
/// pulling serde into the hot path for {"cmd": "..."}-shaped requests.
fn body_field<'a>(body: &'a str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let at = body.find(&pat)? + pat.len();
    let rest = &body[at..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('u') => {
                    let hex: String = (&mut chars).take(4).collect();
                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(n) {
                            out.push(ch);
                        }
                    }
                }
                Some(other) => out.push(other),
                None => break,
            },
            '"' => return Some(out),
            c => out.push(c),
        }
    }
    None
}

fn shell(stream: TcpStream, body: &[u8]) -> std::io::Result<()> {
    let body = String::from_utf8_lossy(body);
    let Some(cmd) = body_field(&body, "cmd") else {
        return respond(stream, 400, "application/json", b"{\"ok\":false,\"out\":\"missing cmd\"}");
    };
    log(&format!("shell$ {cmd}"));
    let started = std::time::Instant::now();
    let child = std::process::Command::new("/system/bin/sh")
        .arg("-c")
        .arg(&cmd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            // Desktop fallback so the page works in local testing too.
            match std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => {
                    let msg = format!("{{\"ok\":false,\"out\":\"spawn: {}\"}}", json_escape(&e.to_string()));
                    return respond(stream, 500, "application/json", msg.as_bytes());
                }
            }
        }
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let code = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.code().unwrap_or(-1),
            Ok(None) => {
                if started.elapsed().as_secs() > SHELL_TIMEOUT_S {
                    let _ = child.kill();
                    break -9;
                }
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            Err(_) => break -1,
        }
    };
    if let Some(o) = stdout.as_mut() {
        let _ = o.read_to_end(&mut out);
    }
    if let Some(e) = stderr.as_mut() {
        let _ = e.read_to_end(&mut err);
    }
    out.truncate(SHELL_OUT_CAP);
    err.truncate(SHELL_OUT_CAP);
    let mut text = String::from_utf8_lossy(&out).to_string();
    if !err.is_empty() {
        text.push_str(&String::from_utf8_lossy(&err));
    }
    let ms = started.elapsed().as_millis();
    log(&format!("shell: exit {code} in {ms}ms, {}B", text.len()));
    let body = format!(
        "{{\"ok\":{},\"code\":{code},\"ms\":{ms},\"out\":\"{}\"}}",
        code == 0,
        json_escape(&text)
    );
    respond(stream, 200, "application/json", body.as_bytes())
}

/// Exec the bundled ffmpeg from nativeLibraryDir — the ONLY place an
/// untrusted_app may exec (measured: `/data/local/tmp/ffmpeg` dies with
/// `Permission denied`, `<libdir>/liboctos.so` runs). The APK ships ffmpeg as
/// `libffmpeg.so` via `MAKEPAD_ANDROID_EXTRA_LIBS`. Body: {"args":"-y -i …"}.
/// Args are split on whitespace — no shell, so no quoting/glob surprises; the
/// output path in the args must sit under an app-writable dir.
fn ffmpeg(stream: TcpStream, body: &[u8]) -> std::io::Result<()> {
    let body = String::from_utf8_lossy(body);
    let Some(args) = body_field(&body, "args") else {
        return respond(stream, 400, "application/json", b"{\"ok\":false,\"out\":\"missing args\"}");
    };
    let Some(Some(lib)) = LIB_DIR.get() else {
        return respond(stream, 200, "application/json", b"{\"ok\":false,\"out\":\"no nativeLibraryDir known\"}");
    };
    let bin = lib.join("libffmpeg.so");
    if !bin.exists() {
        let msg = format!(
            "{{\"ok\":false,\"out\":\"libffmpeg.so not bundled at {} — rebuild with MAKEPAD_ANDROID_EXTRA_LIBS\"}}",
            json_escape(&bin.display().to_string())
        );
        return respond(stream, 200, "application/json", msg.as_bytes());
    }
    let argv: Vec<String> = args.split_whitespace().map(str::to_string).collect();
    log(&format!("ffmpeg$ {args}"));
    let started = std::time::Instant::now();
    let out = std::process::Command::new(&bin)
        .args(&argv)
        .arg("-nostdin")
        .stdin(std::process::Stdio::null())
        .output();
    match out {
        Ok(o) => {
            let ms = started.elapsed().as_millis();
            let mut text = String::from_utf8_lossy(&o.stderr).to_string();
            text.truncate(SHELL_OUT_CAP);
            let code = o.status.code().unwrap_or(-1);
            log(&format!("ffmpeg: exit {code} in {ms}ms"));
            let msg = format!(
                "{{\"ok\":{},\"code\":{code},\"ms\":{ms},\"out\":\"{}\"}}",
                o.status.success(),
                json_escape(&text)
            );
            respond(stream, 200, "application/json", msg.as_bytes())
        }
        Err(e) => {
            let msg = format!("{{\"ok\":false,\"out\":\"exec: {}\"}}", json_escape(&e.to_string()));
            respond(stream, 200, "application/json", msg.as_bytes())
        }
    }
}

fn read_api_key() -> Option<String> {
    if let Ok(k) = std::env::var("OPENAI_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    std::fs::read_to_string("/data/local/tmp/oai_key")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Core image generation, shared by /imagegen and the /concept loop: POST to
/// gpt-image-2, decode, write a unique gen_*.png, return (path, bytes, secs).
fn generate_image(prompt: &str, size: &str, quality: &str) -> Result<(std::path::PathBuf, usize, u64), String> {
    let key = read_api_key().ok_or_else(|| "no API key (push /data/local/tmp/oai_key)".to_string())?;
    log(&format!("imagegen: {} ({size}, {quality})", prompt.chars().take(60).collect::<String>()));
    let started = std::time::Instant::now();
    let req = serde_json::json!({
        "model": "gpt-image-2", "prompt": prompt, "size": size, "quality": quality, "n": 1
    });
    let png = std::thread::spawn(move || -> Result<Vec<u8>, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(async move {
            let client = reqwest::Client::new();
            let resp = client
                .post("https://api.openai.com/v1/images/generations")
                .bearer_auth(key)
                .json(&req)
                .timeout(std::time::Duration::from_secs(300))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status();
            let text = resp.text().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("HTTP {status}: {}", text.chars().take(300).collect::<String>()));
            }
            let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let b64 = v["data"][0]["b64_json"].as_str().ok_or("no b64_json in response")?;
            b64_decode(b64).ok_or_else(|| "bad base64".to_string())
        })
    })
    .join()
    .unwrap_or_else(|_| Err("worker panicked".into()))?;
    let secs = started.elapsed().as_secs();
    let seq = GEN_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!("gen_{}_{}.png", now_stamp(), seq);
    let path = OUT_DIR.get().unwrap().join(&name);
    std::fs::write(&path, &png).map_err(|e| format!("write: {e}"))?;
    log(&format!("imagegen: {} bytes -> {} in {secs}s", png.len(), path.display()));
    Ok((path, png.len(), secs))
}

fn imagegen(stream: TcpStream, body: &[u8]) -> std::io::Result<()> {
    let body = String::from_utf8_lossy(body);
    let Some(prompt) = body_field(&body, "prompt") else {
        return respond(stream, 400, "application/json", b"{\"ok\":false,\"out\":\"missing prompt\"}");
    };
    let size = body_field(&body, "size").unwrap_or_else(|| "1024x1024".into());
    let quality = body_field(&body, "quality").unwrap_or_else(|| "low".into());
    kv("imagegen", "running");
    match generate_image(&prompt, &size, &quality) {
        Ok((path, bytes, secs)) => {
            kv("imagegen", format!("done in {secs}s"));
            let msg = format!(
                "{{\"ok\":true,\"path\":\"{}\",\"bytes\":{bytes},\"secs\":{secs}}}",
                json_escape(&path.display().to_string())
            );
            respond(stream, 200, "application/json", msg.as_bytes())
        }
        Err(e) => {
            kv("imagegen", format!("failed: {}", e.chars().take(80).collect::<String>()));
            log(&format!("imagegen FAILED: {e}"));
            let msg = format!("{{\"ok\":false,\"out\":\"{}\"}}", json_escape(&e));
            respond(stream, 200, "application/json", msg.as_bytes())
        }
    }
}

/// Standard-alphabet base64 encode (for embedding an image in a vision request).
fn b64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// The phone judges its OWN graphic: send the image to a vision model with a
/// rubric and parse `SCORE: <n>` plus a one-line improvement suggestion. This
/// is what puts the 9/10 gate on the device instead of a human.
fn vision_score(path: &std::path::Path, rubric: &str) -> Result<(u32, String, String), String> {
    let key = read_api_key().ok_or_else(|| "no API key".to_string())?;
    let bytes = std::fs::read(path).map_err(|e| format!("read image: {e}"))?;
    let data_uri = format!("data:image/png;base64,{}", b64_encode(&bytes));
    let instruction = format!(
        "You are a strict art director scoring concept key-art for a premium movie box-office app. Rubric: {rubric}. Be critical; reserve 9 and 10 for genuinely production-ready art. Reply on ONE line EXACTLY as: SCORE: <integer 0-10> | <one-sentence critique> | IMPROVE: <one short phrase to add to the image prompt to raise the score>"
    );
    let req = serde_json::json!({
        "model": "gpt-4o",
        "max_tokens": 220,
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": instruction},
            {"type": "image_url", "image_url": {"url": data_uri}}
        ]}]
    });
    let content = std::thread::spawn(move || -> Result<String, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(async move {
            let client = reqwest::Client::new();
            let resp = client
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(key)
                .json(&req)
                .timeout(std::time::Duration::from_secs(120))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status();
            let text = resp.text().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("HTTP {status}: {}", text.chars().take(200).collect::<String>()));
            }
            let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "no content in vision response".to_string())
        })
    })
    .join()
    .unwrap_or_else(|_| Err("scorer panicked".into()))?;
    let score = content
        .split("SCORE:")
        .nth(1)
        .and_then(|s| s.trim_start().split(|c: char| !c.is_ascii_digit()).find(|t| !t.is_empty()))
        .and_then(|d| d.parse::<u32>().ok())
        .ok_or_else(|| format!("no score parsed from: {}", content.chars().take(120).collect::<String>()))?;
    let critique = content.split('|').nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
    let improve = content.split("IMPROVE:").nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
    Ok((score.min(10), critique, improve))
}

/// Score the ACTUAL rendered app screen (not concept art) as a mobile-UX
/// reviewer. Same gpt-4o vision call as `vision_score`, different rubric.
fn ux_score(path: &std::path::Path) -> Result<(u32, String, String), String> {
    let key = read_api_key().ok_or_else(|| "no API key".to_string())?;
    let bytes = std::fs::read(path).map_err(|e| format!("read image: {e}"))?;
    let data_uri = format!("data:image/png;base64,{}", b64_encode(&bytes));
    let instruction = "You are a ruthless senior mobile UI/UX designer reviewing a SCREENSHOT of a running movie box-office app on a phone. Score its VISUAL DESIGN + UX from 0-10 (reserve 9-10 for App-Store-featured quality; most screens are 4-6). Judge visual hierarchy, spacing/rhythm, contrast/legibility, use of imagery, typography, polish. Reply on ONE line EXACTLY as: SCORE: <integer 0-10> | <one-sentence critique> | IMPROVE: <one concrete, buildable change to the card to raise the score>";
    let req = serde_json::json!({
        "model": "gpt-4o",
        "max_tokens": 220,
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": instruction},
            {"type": "image_url", "image_url": {"url": data_uri}}
        ]}]
    });
    let content = std::thread::spawn(move || -> Result<String, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(async move {
            let client = reqwest::Client::new();
            let resp = client
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(key)
                .json(&req)
                .timeout(std::time::Duration::from_secs(120))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status();
            let text = resp.text().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("HTTP {status}: {}", text.chars().take(200).collect::<String>()));
            }
            let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "no content in ux response".to_string())
        })
    })
    .join()
    .unwrap_or_else(|_| Err("ux scorer panicked".into()))?;
    let score = content
        .split("SCORE:")
        .nth(1)
        .and_then(|s| s.trim_start().split(|c: char| !c.is_ascii_digit()).find(|t| !t.is_empty()))
        .and_then(|d| d.parse::<u32>().ok())
        .ok_or_else(|| format!("no score parsed from: {}", content.chars().take(120).collect::<String>()))?;
    let critique = content.split('|').nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
    let improve = content.split("IMPROVE:").nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
    Ok((score.min(10), critique, improve))
}

/// Arm a framebuffer capture BEFORE the display's redraw. Call this on the UI
/// thread immediately before `cx.redraw_all()` so the guaranteed full draw
/// dumps the PNG — reliable even when the UI then idles on cached images (the
/// case that made a post-hoc re-capture return 0 bytes). `spawn_ux_critic`
/// disarms it after the frame settles.
pub fn arm_capture(round: u32) {
    if let Some(out) = OUT_DIR.get() {
        let _ = std::fs::create_dir_all(out);
        let path = out.join(format!("ux_round_{round}.png"));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("MAKEPAD_WRITE_FRAMEBUFFER_PNG", &path);
    }
}

/// The VISUAL half of the dev loop's self-critic: after a generated card
/// renders clean, capture the REAL framebuffer, have a vision model score the
/// UX, and (if below bar) push an instructive finding into the same
/// `DEV_FINDINGS` queue the correctness critic uses — so the agent regenerates
/// to raise the score. Runs on its own thread because scoring is a network call
/// and the capture must wait for a draw. This is the phone judging its own
/// pixels, not the host.
pub fn spawn_ux_critic(round: u32) {
    std::thread::spawn(move || {
        let Some(out) = OUT_DIR.get() else { return };
        let path = out.join(format!("ux_round_{round}.png"));
        log(&format!("[ux-critic] round {round}: letting the frame settle (backdrop + posters), then scoring…"));
        // The capture was ARMED on the UI thread by `arm_capture` BEFORE the
        // display's redraw_all, so that guaranteed full draw already wrote the
        // PNG (works even when the UI then goes idle on cached images). Any
        // image-load redraws overwrite it with the settled frame. Give those a
        // few seconds, then disarm and read whatever the last draw left.
        std::thread::sleep(std::time::Duration::from_millis(5000));
        std::env::remove_var("MAKEPAD_WRITE_FRAMEBUFFER_PNG");
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        log(&format!("[ux-critic] round {round}: captured {bytes} bytes — scoring the UX…"));
        if bytes == 0 {
            log(&format!("[ux-critic] round {round}: no frame captured (UI idle) — skipping score"));
            return;
        }
        match ux_score(&path) {
            Ok((score, critique, improve)) => {
                kv("ux_score", &format!("{score}/10 (round {round})"));
                log(&format!("[ux-critic] round {round}: {score}/10 — {critique}"));
                if score < 8 && round < 8 {
                    let finding = format!(
                        "VISUAL CRITIC — a vision model scored your ACTUAL rendered screen {score}/10. Critique: {critique} Highest-impact fix: {improve} Re-output the FULL improved card between BEGIN_CARD and END_CARD; KEEP the real movie data and poster URLs, only improve the design (layout, spacing, hierarchy, contrast, imagery)."
                    );
                    if let Ok(mut q) = crate::DEV_FINDINGS.lock() {
                        q.push(finding);
                    }
                    makepad_widgets::SignalToUI::set_ui_signal();
                } else {
                    log(&format!("[ux-critic] round {round}: PASS at {score}/10 — loop may stop"));
                }
            }
            Err(e) => log(&format!("[ux-critic] round {round}: score failed — {e}")),
        }
    });
}

/// The phone designs its OWN concept-art prompt (optionally improving on a
/// prior critiqued attempt). Used by /concept when given a brief instead of a
/// fixed prompt — the device authors the visual concept, not the caller.
fn design_prompt(brief: &str, prev: Option<(&str, &str)>) -> Result<String, String> {
    let key = read_api_key().ok_or_else(|| "no API key".to_string())?;
    let mut ask = format!(
        "You are an award-winning concept artist and creative director. Design ONE vivid, specific image-generation prompt for the hero key-art of this product: {brief}. It must be premium, cinematic, production-ready, with a single clear focal point, strong contrast, and clean empty space at the top for a title; specify absolutely no text or letters. Reply with ONLY the prompt text as one paragraph — no preamble, no quotes."
    );
    if let Some((last, critique)) = prev {
        ask.push_str(&format!(
            " Your previous prompt was: {last}. An art director scored it below bar: {critique}. Design a BETTER, meaningfully different concept that fixes that — a bolder single focal point and cleaner composition."
        ));
    }
    let req = serde_json::json!({
        "model": "gpt-4o", "max_tokens": 400,
        "messages": [{"role": "user", "content": ask}]
    });
    let content = std::thread::spawn(move || -> Result<String, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(async move {
            let client = reqwest::Client::new();
            let resp = client
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(key)
                .json(&req)
                .timeout(std::time::Duration::from_secs(90))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status();
            let text = resp.text().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("HTTP {status}: {}", text.chars().take(200).collect::<String>()));
            }
            let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.trim().to_string())
                .ok_or_else(|| "no content in design response".to_string())
        })
    })
    .join()
    .unwrap_or_else(|_| Err("designer panicked".into()))?;
    Ok(content)
}

fn score(stream: TcpStream, body: &[u8]) -> std::io::Result<()> {
    let body = String::from_utf8_lossy(body);
    let Some(path) = body_field(&body, "path") else {
        return respond(stream, 400, "application/json", b"{\"ok\":false,\"out\":\"missing path\"}");
    };
    let rubric = body_field(&body, "rubric").unwrap_or_else(|| {
        "composition, relevance to a movie box-office app, hi-def polish, usable as an app hero with clean space, no text artifacts".into()
    });
    log(&format!("score: {}", path.chars().take(60).collect::<String>()));
    match vision_score(std::path::Path::new(&path), &rubric) {
        Ok((s, crit, imp)) => {
            log(&format!("score: {s}/10 — {crit}"));
            let msg = format!(
                "{{\"ok\":true,\"score\":{s},\"critique\":\"{}\",\"improve\":\"{}\"}}",
                json_escape(&crit), json_escape(&imp)
            );
            respond(stream, 200, "application/json", msg.as_bytes())
        }
        Err(e) => {
            let msg = format!("{{\"ok\":false,\"out\":\"{}\"}}", json_escape(&e));
            respond(stream, 200, "application/json", msg.as_bytes())
        }
    }
}

/// Autonomous concept loop, entirely on-device: generate -> the phone scores
/// itself -> if below threshold, fold the model's OWN improvement suggestion
/// back into the prompt and retry. Runs on its own connection thread, so the
/// monitor page keeps streaming the score of each attempt live while it works.
fn concept(stream: TcpStream, body: &[u8]) -> std::io::Result<()> {
    let body = String::from_utf8_lossy(body);
    // Two modes: a fixed "prompt" (fold the critique in), or a high-level
    // "brief" — in which case the PHONE designs its own concept-art prompt each
    // round and redesigns it from its own critique. brief mode is full autonomy.
    let brief = body_field(&body, "brief");
    let fixed = body_field(&body, "prompt");
    if brief.is_none() && fixed.is_none() {
        return respond(stream, 400, "application/json", b"{\"ok\":false,\"out\":\"need prompt or brief\"}");
    }
    let threshold: u32 = body_field(&body, "threshold").and_then(|s| s.parse().ok()).unwrap_or(9);
    let size = body_field(&body, "size").unwrap_or_else(|| "1536x1024".into());
    let quality = body_field(&body, "quality").unwrap_or_else(|| "high".into());
    let max_tries: u32 = body_field(&body, "max_tries").and_then(|s| s.parse().ok()).unwrap_or(6);
    let rubric = body_field(&body, "rubric").unwrap_or_else(|| {
        "composition, relevance to a premium movie box-office app, hi-def cinematic polish, usable as an app hero with clean sky for a title, no text or letters".into()
    });
    log(&format!(
        "[concept] start ({} mode): target {threshold}/10, up to {max_tries} tries",
        if brief.is_some() { "phone-designed" } else { "fixed" }
    ));
    kv("concept", "running");
    let mut best: Option<(u32, std::path::PathBuf)> = None;
    let mut history: Vec<String> = Vec::new();
    let mut last: Option<(String, String)> = None; // (prompt, critique) for redesign
    let mut folded = fixed.clone().unwrap_or_default();
    for i in 1..=max_tries {
        let prompt = if let Some(b) = &brief {
            match design_prompt(b, last.as_ref().map(|(p, c)| (p.as_str(), c.as_str()))) {
                Ok(p) => {
                    log(&format!("[concept] try {i}: phone designed its own concept — {}", p.chars().take(100).collect::<String>()));
                    p
                }
                Err(e) => { log(&format!("[concept] try {i} design FAILED: {e}")); history.push(format!("try {i}: design failed")); continue; }
            }
        } else {
            folded.clone()
        };
        log(&format!("[concept] try {i}/{max_tries}: generating…"));
        let (path, _bytes, gsecs) = match generate_image(&prompt, &size, &quality) {
            Ok(r) => r,
            Err(e) => { log(&format!("[concept] try {i} gen FAILED: {e}")); history.push(format!("try {i}: gen failed")); continue; }
        };
        let (s, crit, imp) = match vision_score(&path, &rubric) {
            Ok(r) => r,
            Err(e) => { log(&format!("[concept] try {i} score FAILED: {e}")); history.push(format!("try {i}: score failed")); continue; }
        };
        log(&format!("[concept] try {i}: SCORE {s}/10 ({gsecs}s) — {crit}"));
        kv("concept", format!("try {i}: {s}/10"));
        history.push(format!("try {i}: {s}/10 ({crit})"));
        if best.as_ref().map(|(bs, _)| s > *bs).unwrap_or(true) {
            best = Some((s, path.clone()));
        }
        if s >= threshold {
            log(&format!("[concept] PASSED at {s}/10 on try {i}"));
            kv("concept", format!("PASSED {s}/10 (try {i})"));
            let msg = format!(
                "{{\"ok\":true,\"passed\":true,\"score\":{s},\"try\":{i},\"path\":\"{}\",\"history\":\"{}\"}}",
                json_escape(&path.display().to_string()), json_escape(&history.join(" ; "))
            );
            return respond(stream, 200, "application/json", msg.as_bytes());
        }
        last = Some((prompt.clone(), crit.clone()));
        if brief.is_none() && !imp.is_empty() {
            folded = format!("{}. {imp}", fixed.clone().unwrap_or_default());
        }
    }
    let (bs, bp) = best.unwrap_or((0, std::path::PathBuf::new()));
    log(&format!("[concept] stopped: best {bs}/10 (target {threshold} not reached)"));
    kv("concept", format!("best {bs}/10 (no pass)"));
    let msg = format!(
        "{{\"ok\":true,\"passed\":false,\"score\":{bs},\"path\":\"{}\",\"history\":\"{}\"}}",
        json_escape(&bp.display().to_string()), json_escape(&history.join(" ; "))
    );
    respond(stream, 200, "application/json", msg.as_bytes())
}

/// octos photographs its OWN screen: set the framebuffer-dump env var, wait for
/// the draw thread to write a PNG on its next frame, unset it, return the path.
/// This is the on-device "eyes" — no adb, no second app; the same process that
/// authors and scores can now see what it rendered.
fn snapshot(stream: TcpStream) -> std::io::Result<()> {
    let out = OUT_DIR.get().unwrap();
    let _ = std::fs::create_dir_all(out);
    let path = out.join(format!("snap_{}.png", now_stamp()));
    let _ = std::fs::remove_file(&path);
    // The draw thread reads this env var each frame (see the capture hook in
    // aichat's draw_pass_to_fullscreen) and dumps the framebuffer to it.
    std::env::set_var("MAKEPAD_WRITE_FRAMEBUFFER_PNG", &path);
    log("snapshot: requested self-capture");
    let started = std::time::Instant::now();
    while !path.exists() && started.elapsed().as_secs() < 10 {
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    std::env::remove_var("MAKEPAD_WRITE_FRAMEBUFFER_PNG");
    if path.exists() {
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        log(&format!("snapshot: captured {} bytes", bytes));
        let msg = format!(
            "{{\"ok\":true,\"path\":\"{}\",\"bytes\":{bytes}}}",
            json_escape(&path.display().to_string())
        );
        respond(stream, 200, "application/json", msg.as_bytes())
    } else {
        log("snapshot: no frame drawn within 10s");
        respond(stream, 200, "application/json", b"{\"ok\":false,\"out\":\"no frame drawn (UI idle?)\"}")
    }
}

fn now_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = s.bytes().filter(|b| !b" \n\r\t".contains(b)).collect();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut chunk = [0u32; 4];
    let mut n = 0;
    for &b in &bytes {
        if b == b'=' {
            break;
        }
        chunk[n] = val(b)?;
        n += 1;
        if n == 4 {
            let v = (chunk[0] << 18) | (chunk[1] << 12) | (chunk[2] << 6) | chunk[3];
            out.extend_from_slice(&[(v >> 16) as u8, (v >> 8) as u8, v as u8]);
            n = 0;
        }
    }
    match n {
        0 => {}
        2 => {
            let v = (chunk[0] << 18) | (chunk[1] << 12);
            out.push((v >> 16) as u8);
        }
        3 => {
            let v = (chunk[0] << 18) | (chunk[1] << 12) | (chunk[2] << 6);
            out.extend_from_slice(&[(v >> 16) as u8, (v >> 8) as u8]);
        }
        _ => return None,
    }
    Some(out)
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn allowed(path: &std::path::Path) -> bool {
    // No traversal segments; must sit under an allowlisted root.
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    ROOTS
        .get()
        .map(|roots| roots.iter().any(|r| path.starts_with(r)))
        .unwrap_or(false)
}

fn file(stream: TcpStream, query: &str) -> std::io::Result<()> {
    let path = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("path="))
        .map(pct_decode)
        .unwrap_or_default();
    let path = std::path::PathBuf::from(path);
    if !allowed(&path) {
        return respond(stream, 403, "text/plain", b"path not under an allowed root");
    }
    match std::fs::read(&path) {
        Ok(bytes) => {
            let ctype = match path.extension().and_then(|e| e.to_str()) {
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("mp4") => "video/mp4",
                Some("json") => "application/json",
                _ => "text/plain; charset=utf-8",
            };
            respond(stream, 200, ctype, &bytes)
        }
        Err(e) => {
            let msg = format!("read {}: {e}", path.display());
            respond(stream, 404, "text/plain", msg.as_bytes())
        }
    }
}

fn gallery(stream: TcpStream) -> std::io::Result<()> {
    let mut items: Vec<(u64, String)> = Vec::new();
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(roots) = ROOTS.get() {
        dirs.extend(roots.iter().cloned());
    }
    dirs.push(std::path::PathBuf::from("/data/local/tmp/mp"));
    if let Some(o) = OUT_DIR.get() {
        dirs.push(o.clone());
    }
    for dir in dirs {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
                if matches!(ext, "png" | "jpg" | "jpeg" | "mp4") {
                    let mtime = e
                        .metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    items.push((mtime, p.display().to_string()));
                }
            }
        }
    }
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.truncate(40);
    let body = format!(
        "[{}]",
        items
            .iter()
            .map(|(t, p)| format!("{{\"mtime\":{t},\"path\":\"{}\"}}", json_escape(p)))
            .collect::<Vec<_>>()
            .join(",")
    );
    respond(stream, 200, "application/json", body.as_bytes())
}

const PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>octos monitor</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  :root { color-scheme: dark; }
  body { margin: 0; background: #101418; color: #cfd8e3; font: 13px/1.45 ui-monospace, Menlo, monospace; }
  header { display: flex; flex-wrap: wrap; gap: 8px; align-items: baseline; padding: 10px 14px; background: #171d24; border-bottom: 1px solid #232b35; position: sticky; top: 0; }
  header b { color: #7ec8ff; font-size: 14px; }
  .chip { background: #1f2733; border: 1px solid #2c3644; border-radius: 12px; padding: 1px 10px; white-space: nowrap; }
  .chip span { color: #8fa3b8; }
  main { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; padding: 12px 14px; }
  @media (max-width: 900px) { main { grid-template-columns: 1fr; } }
  section { background: #151b22; border: 1px solid #232b35; border-radius: 8px; overflow: hidden; }
  section h2 { margin: 0; padding: 6px 10px; font-size: 12px; color: #8fa3b8; background: #1a212a; border-bottom: 1px solid #232b35; font-weight: 600; }
  #log { height: 320px; overflow-y: auto; padding: 8px 10px; white-space: pre-wrap; word-break: break-all; }
  #log .e { color: #ff9a8a; }
  .pad { padding: 10px; }
  input[type=text], select { background: #0d1116; color: #cfd8e3; border: 1px solid #2c3644; border-radius: 6px; padding: 6px 8px; font: inherit; }
  input[type=text] { width: calc(100% - 90px); }
  button { background: #234; color: #9ecbff; border: 1px solid #35608a; border-radius: 6px; padding: 6px 12px; font: inherit; cursor: pointer; }
  button:hover { background: #2a4a6b; }
  pre { margin: 8px 0 0; max-height: 260px; overflow: auto; white-space: pre-wrap; word-break: break-all; background: #0d1116; border: 1px solid #232b35; border-radius: 6px; padding: 8px; }
  #gal { display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 8px; }
  #gal a { display: block; }
  #gal img, #gal video { width: 100%; border-radius: 6px; border: 1px solid #2c3644; display: block; }
  #gal .cap { color: #718396; font-size: 10px; word-break: break-all; }
  #genout img { max-width: 100%; border-radius: 8px; margin-top: 8px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
</style></head><body>
<header><b>octos monitor</b><span id="chips"></span></header>
<main>
  <section style="grid-column: 1 / -1;"><h2>live log</h2><div id="log"></div></section>
  <section><h2>shell (phone, app context)</h2><div class="pad">
    <div class="row"><input id="cmd" type="text" placeholder="e.g. id; ls /data/local/tmp/mp" onkeydown="if(event.key==='Enter')runsh()"><button onclick="runsh()">run</button></div>
    <pre id="shout">—</pre></div></section>
  <section><h2>image generation (phone → OpenAI)</h2><div class="pad">
    <div class="row"><input id="prompt" type="text" placeholder="prompt"></div>
    <div class="row" style="margin-top:8px">
      <select id="size"><option>1024x1024</option><option>1024x1536</option><option>1536x1024</option></select>
      <select id="quality"><option>low</option><option>medium</option><option>high</option></select>
      <button onclick="gen()">generate</button></div>
    <div id="genout">—</div></div></section>
  <section><h2>ffmpeg (bundled, app context)</h2><div class="pad">
    <div class="row"><input id="ffargs" type="text" placeholder='-y -f lavfi -i testsrc=duration=2:size=640x360:rate=30 /data/data/dev.makepad.octos_app/files/monitor/t.mp4' onkeydown="if(event.key==='Enter')ff()"><button onclick="ff()">run</button></div>
    <pre id="ffout">—</pre></div></section>
  <section style="grid-column: 1 / -1;"><h2>artifacts</h2><div class="pad"><div id="gal"></div></div></section>
</main>
<script>
const $ = id => document.getElementById(id);
function chips(o) {
  $('chips').innerHTML = Object.entries(o).map(([k,v]) =>
    `<span class="chip"><span>${k}</span> ${String(v).slice(0,80)}</span>`).join(' ');
}
async function poll() {
  try { chips(await (await fetch('status.json')).json()); } catch(e) {}
  setTimeout(poll, 1500);
}
poll();
const es = new EventSource('events');
es.onmessage = ev => {
  const el = document.createElement('div');
  if (/FAILED|failed|error|denied|timed out/.test(ev.data)) el.className = 'e';
  el.textContent = ev.data;
  const log = $('log');
  const stick = log.scrollTop + log.clientHeight >= log.scrollHeight - 8;
  log.appendChild(el);
  while (log.childNodes.length > 800) log.removeChild(log.firstChild);
  if (stick) log.scrollTop = log.scrollHeight;
};
async function runsh() {
  $('shout').textContent = '…';
  try {
    const r = await (await fetch('shell', { method: 'POST', body: JSON.stringify({ cmd: $('cmd').value }) })).json();
    $('shout').textContent = `[exit ${r.code} · ${r.ms}ms]\n` + r.out;
  } catch(e) { $('shout').textContent = String(e); }
}
async function gen() {
  $('genout').textContent = 'generating… (the phone is calling the API)';
  try {
    const body = JSON.stringify({ prompt: $('prompt').value, size: $('size').value, quality: $('quality').value });
    const r = await (await fetch('imagegen', { method: 'POST', body })).json();
    if (r.ok) $('genout').innerHTML =
      `<div>${r.bytes} bytes in ${r.secs}s</div><img src="file?path=${encodeURIComponent(r.path)}">`;
    else $('genout').textContent = 'failed: ' + r.out;
  } catch(e) { $('genout').textContent = String(e); }
  refreshGal();
}
async function ff() {
  $('ffout').textContent = '…';
  try {
    const r = await (await fetch('ffmpeg', { method: 'POST', body: JSON.stringify({ args: $('ffargs').value }) })).json();
    $('ffout').textContent = `[exit ${r.code} · ${r.ms}ms]\n` + (r.out || '(no stderr)');
  } catch(e) { $('ffout').textContent = String(e); }
  refreshGal();
}
async function refreshGal() {
  try {
    const items = await (await fetch('gallery.json')).json();
    $('gal').innerHTML = items.map(it => {
      const u = 'file?path=' + encodeURIComponent(it.path);
      const media = it.path.endsWith('.mp4')
        ? `<video src="${u}" controls muted></video>` : `<a href="${u}" target="_blank"><img src="${u}"></a>`;
      return `<div>${media}<div class="cap">${it.path.split('/').pop()}</div></div>`;
    }).join('');
  } catch(e) {}
}
refreshGal();
setInterval(refreshGal, 5000);
</script></body></html>
"#;
