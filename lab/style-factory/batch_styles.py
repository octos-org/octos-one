#!/usr/bin/env python3
"""The style factory: recipe -> mockup -> HTML twin -> Splash card, all judged.

Per specimen:
  1. gpt-image-2 renders the recipe as a mockup (paired split when photo_bg).
  2. claude (headless) writes an HTML twin of the mockup — LIVE data, assets
     from the local server — rendered in octos-one's own webview via the
     seed.html trigger, screenshotted, judged against the mockup.
  3. claude translates the winning design to a Splash L0 card constrained by
     the real catalog; l0validate gates it (diagnostics fed back, 2 rounds);
     rendered via SEED_L0 with a seeded ledger, screenshotted, judged.
  4. ledger.jsonl gets the full record: recipe, paths, scores, judge notes.

Resume-safe: a specimen with a ledger entry is skipped, so the batch can be
killed and relaunched. The phone is the scarce resource — every adb step
waits for the device and re-asserts the reverse tunnels, because today's USB
dropped three times.
"""
import base64
import io
import json
import re
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

BASE = Path(__file__).resolve().parent
ASSETS = BASE.parent / "pipeline-v2"          # the :8899 server root
KEY = Path("/private/tmp/claude-501/-Users-yuechen-home-Splash/10acdae2-9f20-4e60-b7b1-7195c1bdb439/scratchpad/oai_key").read_text().strip()
PROXY = "http://127.0.0.1:7897"
ADB = str(Path.home() / "Library/Android/sdk/platform-tools/adb")
DEVICE = "bf0a4730"
PKG = "dev.makepad.octos_app"
CARDS = f"/storage/emulated/0/Android/media/{PKG}/cards"
SPLASH = Path.home() / "home/Splash"
ACCEPT = 7

CONTRACTS = {
    "weather": """source place sys.geocode(name: state.city)  # answers lat, lon, name
source now  sys.weather(lat: place.lat, lon: place.lon, fields: [temp, feels, cond])
source week sys.weather(lat: place.lat, lon: place.lon, days: 7, fields: [dayname, hi, lo, cond], aggregate: [min_lo, max_hi])
source scene sys.photo(query: state.mood)   # only for photo backgrounds
state city { shape: text, initial: "Tokyo" }""",
    "news": """source lead sys.news(count: 1, fields: [id, title, author, points, comments])
source feed sys.news(count: 5, fields: [id, title, author, points])
source scene sys.photo(query: state.mood)   # only for photo backgrounds""",
    "stock": """source movers sys.movers(count: 5, fields: [ticker, name, last, change, pct])
source scene sys.photo(query: state.mood)   # only for photo backgrounds""",
    "quake": """source lead  sys.quakes(count: 1, fields: [id, mag, place, depth, ago])
source feed  sys.quakes(count: 5, offset: 1, fields: [id, mag, place, ago])
source scene sys.photo(query: state.mood)   # only for photo backgrounds""",
}

HTML_DATA = {
    "weather": 'fetch open-meteo for Tokyo: https://api.open-meteo.com/v1/forecast?latitude=35.6895&longitude=139.6917&current=temperature_2m,weather_code&daily=temperature_2m_max,temperature_2m_min,weather_code&timezone=Asia%2FTokyo',
    "news": 'fetch the live HN front page: https://hn.algolia.com/api/v1/search?tags=front_page&hitsPerPage=6 (hits[].title/author/points/num_comments)',
    "stock": 'fetch live quotes from https://query1.finance.yahoo.com/v8/finance/chart/{SYM}?range=1d&interval=1d for NVDA, AAPL, MSFT, TSLA, AMD (meta.regularMarketPrice, meta.chartPreviousClose); if a fetch fails render the row with an em-dash, never blank',
    "quake": 'fetch the live USGS feed https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/2.5_day.geojson (features[].properties.mag/place/time, newest first)',
}


CONSTRUCTORS = Path("/Users/yuechen/home/Splash/docs/ui-l0-constructors.toml").read_text()


def sh(*args, timeout=120, **kw):
    return subprocess.run(list(args), capture_output=True, text=True, timeout=timeout, **kw)


def adb(*args, timeout=60):
    return sh(ADB, "-s", DEVICE, *args, timeout=timeout)


def wait_device():
    for _ in range(60):
        if adb("get-state").stdout.strip() == "device":
            adb("reverse", "tcp:8899", "tcp:8899")
            adb("reverse", "tcp:7897", "tcp:7897")   # the Mac tunnel IS the internet
            return True
        time.sleep(5)
    return False


def log(sid, msg):
    print(f"[{sid}] {msg}", flush=True)


def imagegen(prompt, size, out_path):
    req = urllib.request.Request(
        "https://api.openai.com/v1/images/generations",
        data=json.dumps({"model": "gpt-image-2", "prompt": prompt,
                         "size": size, "quality": "medium"}).encode(),
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({"https": PROXY}))
    with opener.open(req, timeout=300) as r:
        d = json.load(r)
    out_path.write_bytes(base64.b64decode(d["data"][0]["b64_json"]))


def claude_text(prompt, timeout=900):
    r = sh("claude", "-p", prompt, "--model", "opus", "--allowedTools", "Read",
           "--output-format", "json", timeout=timeout, cwd=BASE)
    return json.loads(r.stdout)["result"]


def strip_fence(text, lang=""):
    m = re.search(rf"```{lang}[^\n]*\n(.*?)```", text, re.S)
    return m.group(1) if m else text


def judge(target, render, extra=""):
    out = claude_text(
        f"Two images. FIRST Read {target} — the TARGET design mockup. THEN Read {render} — "
        f"an implementation rendered live on a phone. IGNORE, as none are the implementation's "
        f"fault: the small floating button and any bottom app chrome; differences in overall "
        f"aspect ratio and any empty space that follows from them; imagery CONTENT the "
        f"implementation could not obtain (the mockup's photographs, avatars and illustrations "
        f"are invented and unavailable); and text that is longer or shorter than the mockup's "
        f"because it is real live data. JUDGE the design system: composition, typographic scale "
        f"and hierarchy, colour relationships, spacing rhythm, shape language, surface treatment"
        f"{extra}. Return ONLY JSON: {{\"fidelity\": 1-10, \"overall\": 1-10, "
        f"\"gaps\": \"<=25 words\"}}")
    m = re.search(r"\{.*\}", out, re.S)
    return json.loads(m.group(0))


def render_html(html_path, shot_path, settle=15):
    if not wait_device():
        raise RuntimeError("device gone")
    adb("push", str(html_path), f"{CARDS}/seed.html")
    adb("shell", "am", "force-stop", PKG)
    adb("shell", "am", "start", "-S", "-n", f"{PKG}/.MakepadApp",
        "--es", "makepad.OCTOS_PROXY", "http://127.0.0.1:7897")
    settle_then_cap(shot_path, grace=settle // 2)


def render_card(card_path, ledger, shot_path, settle=75):
    if not wait_device():
        raise RuntimeError("device gone")
    # Bounce the tunnels: a rotted adbd reverse socket fails every fetch in
    # the fresh process, which renders a perfect em-dash skeleton — the
    # dominant false low score of the first 65 specimens.
    adb("reverse", "--remove", "tcp:7897"); adb("reverse", "tcp:7897", "tcp:7897")
    adb("reverse", "--remove", "tcp:8899"); adb("reverse", "tcp:8899", "tcp:8899")
    led = BASE / "tmp_ledger.json"
    led.write_text(json.dumps(ledger))
    adb("push", str(card_path), f"{CARDS}/batch.card")
    adb("push", str(led), f"{CARDS}/batch.json")
    adb("shell", "am", "force-stop", PKG)
    adb("shell", "logcat", "-c")
    adb("shell", "am", "start", "-S", "-n", f"{PKG}/.MakepadApp",
        "--es", "makepad.OCTOS_PROXY", "http://127.0.0.1:7897",
        "--es", "makepad.SEED_L0_FILE", f"{CARDS}/batch.card",
        "--es", "makepad.SEED_L0_DATA", f"{CARDS}/batch.json")
    time.sleep(8)
    logtail = adb("logcat", "-d").stdout
    if "SEED_L0 realize failed" in logtail:
        why = [l for l in logtail.splitlines() if "SEED_L0" in l][-1]
        return why[:400]
    settle_then_cap(shot_path, grace=6, timeout=max(settle, 45))
    return None


def settle_then_cap(path, grace=6, timeout=45):
    """Wait until the frame stops changing (<0.2% pixels over 3 consecutive
    grabs), then save. Fixed sleeps captured loading skeletons whenever a
    fetch was slow — the judge then scored em-dashes, not design."""
    import numpy as np
    from PIL import Image
    def grab():
        raw = subprocess.run([ADB, "-s", DEVICE, "exec-out", "screencap", "-p"],
                             capture_output=True, timeout=60).stdout
        im = Image.open(io.BytesIO(raw))
        return im.crop((0, 90, im.width, im.height - 60))
    time.sleep(grace)
    prev, stable = grab(), 0
    for _ in range(timeout):
        time.sleep(1)
        cur = grab()
        d = np.abs(np.asarray(prev, dtype=np.int16) - np.asarray(cur, dtype=np.int16))
        stable = stable + 1 if float((d.max(axis=2) > 24).mean()) <= 0.002 else 0
        prev = cur
        if stable >= 2:
            break
    prev.save(path)


def screencap(path):
    raw = subprocess.run([ADB, "-s", DEVICE, "exec-out", "screencap", "-p"],
                         capture_output=True, timeout=60).stdout
    from PIL import Image
    im = Image.open(io.BytesIO(raw))
    im.crop((0, 90, im.width, im.height - 60)).save(path)


def validate(card_path):
    # ABSOLUTE: cargo runs with cwd=SPLASH, so a relative path silently
    # resolves against the wrong directory and the validator reports a
    # missing `view root` for a card that is perfectly fine.
    r = sh("cargo", "run", "-q", "-p", "splash-ui-l0", "--example", "l0validate",
           "--", str(Path(card_path).resolve()), cwd=SPLASH, timeout=300)
    try:
        return json.loads(r.stdout.strip().splitlines()[-1])
    except Exception:
        return {"ok": False, "diagnostics": [r.stdout[-400:] + r.stderr[-200:]]}


def run_specimen(r, outdir):
    sid = r["id"]
    rec = {"id": sid, "recipe": r}
    style_line = (
        f"{r['art']} "
        f"COMPOSITION: {r['layout']}. "
        f"TYPE: {r['type_class'].replace('_', ' ')} at a {r['ratio']} scale ratio, "
        f"{r['hierarchy']} hierarchy. "
        f"GEOMETRY: {r['geometry'].replace('_', ' ')}. "
        f"COLOUR: {r['harmony'].replace('_', ' ')} harmony on a {r['key'].replace('_', ' ')} ground. "
        f"CONTRAST carried primarily by {r['contrast'].replace('_', ' ')}. "
        f"FIGURE-GROUND: {r['figure_ground'].replace('_', ' ')}. "
        f"ORNAMENT: {r['ornament'].replace('_', ' ')}. "
        f"SPACING: {r['density']}. "
        f"IMAGERY: {r['media'].replace('_', ' ')}.")

    # ── 1. the mockup ─────────────────────────────────────────────────────
    mock = outdir / f"{sid}-mockup.png"
    if not mock.exists():
        if r["photo_bg"]:
            prompt = (f"Split-panel design sheet, two equal panels, hard vertical divider. "
                      f"LEFT: a clean portrait photograph suited to a {r['domain']} app backdrop, "
                      f"no text, no UI, cinematic, slightly dark. RIGHT: {r['content']}, using "
                      f"EXACTLY the left panel's photograph as its full-bleed background. "
                      f"STYLE: {style_line}. No device frame, no watermark, no logos.")
            paired = outdir / f"{sid}-paired.png"
            imagegen(prompt, "1792x1920", paired)
            from PIL import Image
            im = Image.open(paired)
            im.crop((0, 0, im.width // 2, im.height)).save(outdir / f"{sid}-bg.png")
            im.crop((im.width // 2, 0, im.width, im.height)).save(mock)
            (ASSETS / f"{sid}-bg.png").write_bytes((outdir / f"{sid}-bg.png").read_bytes())
        else:
            imagegen(f"{r['content']}, portrait, single screen. STYLE: {style_line}. "
                     f"The design fills the FULL HEIGHT of a tall phone screen, edge to edge, "
                     f"with no empty band at the bottom. No device frame, no watermark, no logos.",
                     "896x1920", mock)
    log(sid, "mockup ok")

    # ── 2. the HTML twin ──────────────────────────────────────────────────
    html = outdir / f"{sid}.html"
    bg_note = (f"The page background image is http://127.0.0.1:8899/{sid}-bg.png (the mockup's "
               f"exact photograph)." if r["photo_bg"] else
               "Flat background — reproduce the mockup's colours in CSS.")
    if not html.exists():
        out = claude_text(
            f"Read {mock} — a {r['domain']} app design mockup. Write ONE self-contained HTML "
            f"document that reproduces this design as faithfully as possible on a phone "
            f"(Chromium webview, viewport meta, html/body margin 0, min-height 100vh, "
            f"padding-top 54px for the status bar, system font stack — Roboto and serif "
            f"fallbacks are available). {bg_note} LIVE data, never hardcoded: {HTML_DATA[r['domain']]} "
            f"— show an em-dash skeleton while loading. Return ONLY the HTML in one ```html fence.")
        html.write_text(strip_fence(out, "html"))
    shot_h = outdir / f"{sid}-html.png"
    if not shot_h.exists():
        render_html(html, shot_h)
    jh = judge(mock, shot_h)
    rec["html"] = {"fidelity": jh.get("fidelity"), "overall": jh.get("overall"), "gaps": jh.get("gaps")}
    log(sid, f"html judged {jh.get('fidelity')}/{jh.get('overall')}")

    # one revision pass if below the gate — OPTIONAL: a timeout here must not
    # kill the specimen, so it degrades to shipping the first draft.
    if (jh.get("fidelity") or 0) < ACCEPT and not (outdir / f"{sid}-html2.png").exists():
        try:
            out = claude_text(
                f"Read {mock} (the TARGET design), Read {shot_h} (the current render), and "
                f"Read {html} (the current HTML source). A design judge said: {jh.get('gaps')!r}. "
                f"Improve the HTML to close those gaps; keep the live data and asset URLs "
                f"identical. Return ONLY the complete improved HTML in one ```html fence.")
            html.write_text(strip_fence(out, "html"))
            shot_h = outdir / f"{sid}-html2.png"
            render_html(html, shot_h)
            jh = judge(mock, shot_h)
            rec["html2"] = {"fidelity": jh.get("fidelity"), "overall": jh.get("overall"), "gaps": jh.get("gaps")}
            log(sid, f"html rev2 {jh.get('fidelity')}/{jh.get('overall')}")
        except Exception as e:  # noqa: BLE001
            log(sid, f"revision skipped: {e}")

    # ── 3. the Splash card ────────────────────────────────────────────────
    card = outdir / f"{sid}.card"
    diags = ""
    for attempt in (1, 2, 3):
        if card.exists() and attempt == 1:
            break
        out = claude_text(
            f"Read {mock} — the design target. Read "
            f"{BASE.parent/'translated/weather-tokyo.card'} and {BASE.parent/'translated/news-photo.card'} "
            f"— two valid L0 cards showing the EXACT dialect (sources, state, copy, views, for-loops, "
            f"when-guards, TextEyebrow/TextHero/TextRow/TextCaption/TextValue/Panel/Card/Row/Col/Rule, "
            f"theme photo|light|dark|glass). Write ONE L0 card for this {r['domain']} design. "
            f"THE CONSTRUCTOR CONTRACT — every role and every argument L0 admits. Do not "
            f"invent arguments; anything not listed here is a validation error:\n"
            f"{CONSTRUCTORS}\n"
            f"Data contract (use ONLY these sources; display every source you declare):\n"
            f"{CONTRACTS[r['domain']]}\n"
            f"Choose the theme mood closest to the mockup ({'photo' if r['photo_bg'] else 'light or dark'}). "
            f"Every number/text must bind a source or copy — never a hardcoded fact. "
            f"A photo background source MUST keep the exact name `scene`. "
            f"{('Previous attempt failed validation: ' + diags) if diags else ''} "
            f"Return ONLY the card source in one ```runl0 fence, first line `# level: L0`.")
        card.write_text(strip_fence(out, "runl0"))
        v = validate(card)
        if v.get("ok"):
            diags = ""
            break
        diags = "; ".join(str(d) for d in v.get("diagnostics", []))[:500]
        log(sid, f"validate attempt {attempt} failed: {diags[:120]}")
    rec["card_valid"] = not diags
    if diags:
        rec["card_diags"] = diags
        return rec

    shot_c = outdir / f"{sid}-card.png"
    ledger = {"scene": f"http://127.0.0.1:8899/{sid}-bg.png"} if r["photo_bg"] else {}
    if r["domain"] == "weather":
        ledger["city"] = "Tokyo"
    err = None
    if not shot_c.exists():
        err = render_card(card, ledger, shot_c)
    if err:
        rec["card_render_error"] = err
        return rec
    jc = judge(mock, shot_c, extra="; the background photo may differ from the target's — judge composition not photo choice")
    # A fidelity <=3 is as often a transient data failure (dead tunnel socket,
    # slow photo service) as a design gap — fresh renders tell them apart.
    for retry in (2, 3):
        if (jc.get("fidelity") or 0) > 3:
            break
        log(sid, f"card retry {retry - 1} (fidelity {jc.get('fidelity')})")
        shot_r = outdir / f"{sid}-card{retry}.png"
        if render_card(card, ledger, shot_r) is None:
            jc2 = judge(mock, shot_r, extra="; the background photo may differ from the target's — judge composition not photo choice")
            log(sid, f"card retry judged {jc2.get('fidelity')}/{jc2.get('overall')}")
            if (jc2.get("fidelity") or 0) > (jc.get("fidelity") or 0):
                rec["card_retry"] = retry - 1
                jc = jc2
    rec["card"] = {"fidelity": jc.get("fidelity"), "overall": jc.get("overall"), "gaps": jc.get("gaps")}
    log(sid, f"card judged {jc.get('fidelity')}/{jc.get('overall')}")
    return rec


def main():
    cards_only = "--cards-only" in sys.argv
    outdir = BASE / "out"
    outdir.mkdir(exist_ok=True)
    ledger_path = BASE / "ledger.jsonl"
    done = set()
    if ledger_path.exists():
        done = {json.loads(l)["id"] for l in open(ledger_path)}
    recipes = [json.loads(l) for l in open(BASE / "recipes.jsonl")]
    if cards_only:
        # Re-render and re-judge existing cards against existing mockups. No
        # image generation, no HTML: the phase-delta measurement.
        todo = [r for r in recipes if (outdir / f"{r['id']}.card").exists()]
        for r in todo:
            for stale in outdir.glob(f"{r['id']}-card*.png"):
                stale.unlink()
        ledger_path.rename(ledger_path.with_suffix(".jsonl.prev"))
    else:
        todo = [r for r in recipes if r["id"] not in done]
    print(f"{len(todo)} specimens to run ({len(done)} already done)", flush=True)
    for r in todo:
        try:
            rec = run_specimen(r, outdir)
        except Exception as e:  # noqa: BLE001 — a specimen must never kill the batch
            rec = {"id": r["id"], "recipe": r, "error": str(e)[:300]}
            log(r["id"], f"ERROR {e}")
        with open(ledger_path, "a") as f:
            f.write(json.dumps(rec) + "\n")
    print("batch complete", flush=True)


if __name__ == "__main__":
    main()
