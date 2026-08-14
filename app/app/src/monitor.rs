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
        let listener = match TcpListener::bind("127.0.0.1:8686") {
            Ok(l) => l,
            Err(e) => {
                log(&format!("monitor: bind failed: {e}"));
                return;
            }
        };
        log("monitor: listening on 127.0.0.1:8686");
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

fn imagegen(stream: TcpStream, body: &[u8]) -> std::io::Result<()> {
    let body = String::from_utf8_lossy(body);
    let Some(prompt) = body_field(&body, "prompt") else {
        return respond(stream, 400, "application/json", b"{\"ok\":false,\"out\":\"missing prompt\"}");
    };
    let size = body_field(&body, "size").unwrap_or_else(|| "1024x1024".into());
    let quality = body_field(&body, "quality").unwrap_or_else(|| "low".into());
    let Some(key) = read_api_key() else {
        return respond(
            stream,
            200,
            "application/json",
            b"{\"ok\":false,\"out\":\"no API key: push /data/local/tmp/oai_key (0644) or set OPENAI_API_KEY\"}",
        );
    };
    log(&format!("imagegen: {} ({size}, {quality})", prompt.chars().take(60).collect::<String>()));
    kv("imagegen", "running");
    let started = std::time::Instant::now();
    let req = serde_json::json!({
        "model": "gpt-image-2", "prompt": prompt, "size": size, "quality": quality, "n": 1
    });
    let result = std::thread::spawn(move || -> Result<Vec<u8>, String> {
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
            let b64 = v["data"][0]["b64_json"]
                .as_str()
                .ok_or("no b64_json in response")?;
            b64_decode(b64).ok_or_else(|| "bad base64".to_string())
        })
    })
    .join()
    .unwrap_or_else(|_| Err("worker panicked".into()));
    match result {
        Ok(png) => {
            let secs = started.elapsed().as_secs();
            let name = format!("gen_{}.png", now_stamp());
            let path = OUT_DIR.get().unwrap().join(&name);
            if let Err(e) = std::fs::write(&path, &png) {
                let msg = format!("{{\"ok\":false,\"out\":\"write: {}\"}}", json_escape(&e.to_string()));
                return respond(stream, 500, "application/json", msg.as_bytes());
            }
            kv("imagegen", format!("done {name} in {secs}s"));
            log(&format!("imagegen: {} bytes -> {} in {secs}s", png.len(), path.display()));
            let msg = format!(
                "{{\"ok\":true,\"path\":\"{}\",\"bytes\":{},\"secs\":{secs}}}",
                json_escape(&path.display().to_string()),
                png.len()
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
