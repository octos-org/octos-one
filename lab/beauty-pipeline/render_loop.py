#!/usr/bin/env python3
"""Render every harvested card on the OnePlus 6T via the SEED_L0 hook.

Resume-safe: a card with an existing shot or a recorded failure is skipped, so
the loop can be killed and restarted at any point. Failures are recorded with
the realize error pulled from logcat — a card that cannot realize is data too
(it must never enter the beautiful-cards training set).

Writes RENDER_DONE when the whole meta list has been attempted.
"""
import io
import json
import subprocess
import sys
import time
from pathlib import Path

from PIL import Image
import numpy as np

BASE = Path(__file__).resolve().parent
ADB = str(Path.home() / "Library/Android/sdk/platform-tools/adb")
DEVICE = "bf0a4730"
PKG = "dev.makepad.octos_app"
CARDS = f"/storage/emulated/0/Android/media/{PKG}/cards"
CROP_TOP, CROP_BOTTOM = 90, 195
LOAD_TIMEOUT, SETTLE_TIMEOUT = 25, 25


def adb(*args, **kw):
    return subprocess.run([ADB, "-s", DEVICE, *args], capture_output=True, **kw)


def grab():
    shot = adb("exec-out", "screencap", "-p").stdout
    img = Image.open(io.BytesIO(shot)).convert("RGB")
    return img.crop((0, CROP_TOP, img.width, img.height - CROP_BOTTOM))


def differs(a, b):
    """Fraction-based: a shimmering gradient or ticking chart never settles
    pixel-perfectly, but under 0.2% moving pixels the card is done drawing."""
    if a.size != b.size:
        return True
    d = np.abs(np.asarray(a, dtype=np.int16) - np.asarray(b, dtype=np.int16))
    return float((d.max(axis=2) > 24).mean()) > 0.002


def dismiss_ime():
    for _ in range(3):
        state = adb("shell", "dumpsys", "input_method").stdout.decode("utf-8", "replace")
        if "mIsInputViewShown=true" not in state:
            return
        adb("shell", "input", "keyevent", "KEYCODE_BACK")
        time.sleep(1)


UNUSED = __import__("re").compile(r'source "([^"]+)" is declared and never read')

# Production seeds state from the query; the seed harness must do the same or
# every geocode gets "" and the card renders em-dashes (measured, not guessed).
# Cities are harvest.py's own list, so every query that names one matches.
CITIES = ("Tokyo London Paris Berlin Madrid Rome Vienna Prague Lisbon Athens Cairo Oslo "
    "Helsinki Warsaw Dublin Zurich Porto Naples Lyon Turin Seville Genoa Nice Basel Ghent "
    "Leeds Bergen Aarhus Graz Brno Kyoto Osaka Seoul Busan Taipei Singapore Bangkok Hanoi "
    "Mumbai Delhi Nairobi Lagos Casablanca Istanbul Dubai Doha Sydney Melbourne Auckland "
    "Toronto Vancouver Montreal Chicago Boston Seattle Denver Austin Miami Havana Lima "
    "Bogota Santiago Quito Reykjavik Tallinn Riga Vilnius Krakow Zagreb Belgrade Sofia").split()
TICKERS = ("NVDA AAPL MSFT GOOG AMZN META TSLA AMD INTC AVGO ORCL CRM ADBE NFLX QCOM TXN "
    "MU AMAT ASML TSM BABA JD PDD NIO XPEV LI UBER ABNB SHOP SQ COIN PLTR SNOW DDOG NET").split()


def ledger_for(query):
    """State seed inferred from the query, mirroring what production injects."""
    q = query.lower()
    led = {"city": "Tokyo"}  # 'here'-style queries: any real city renders the same design
    for c in CITIES:
        if c.lower() in q:
            led["city"] = c
            break
    for t in TICKERS:
        if t.lower() in q.split() or t in query:
            led["ticker"] = t
            led["selected"] = t
            break
    return led


def strip_sources(src, names):
    """Remove whole `source <name> …(…)` declarations (multi-line, paren-balanced).

    Render-only accommodation: an unused source never reaches the screen, so the
    pixels are identical to the original card's — but the seed path lints it
    fatally where production would repair. The training sequence stays untouched.
    """
    out, lines, i = [], src.splitlines(), 0
    while i < len(lines):
        line = lines[i]
        stripped = line.lstrip()
        name = stripped.split()[1] if stripped.startswith("source ") and len(stripped.split()) > 1 else None
        if name in names:
            depth = line.count("(") - line.count(")")
            i += 1
            while depth > 0 and i < len(lines):
                depth += lines[i].count("(") - lines[i].count(")")
                i += 1
            continue
        out.append(line)
        i += 1
    return "\n".join(out) + "\n"


def render(card_id, query, source_override=None):
    # The phone reaches the open internet only through the Mac's VPN proxy
    # (adb reverse + global http_proxy). reverse dies with a USB blip, so
    # re-assert per card — idempotent and ~10ms.
    adb("reverse", "tcp:7897", "tcp:7897")
    remote_card, remote_data = f"{CARDS}/beauty.card", f"{CARDS}/beauty.json"
    if source_override is None:
        adb("push", str(BASE / "cards" / f"{card_id}.card"), remote_card)
    else:
        tmp = BASE / "stripped.card"
        tmp.write_text(source_override)
        adb("push", str(tmp), remote_card)
    ledger = BASE / "ledger.json"
    ledger.write_text(json.dumps(ledger_for(query)))
    adb("push", str(ledger), remote_data)
    adb("shell", "am", "force-stop", PKG)
    adb("logcat", "-c")
    adb("shell", "input", "keyevent", "KEYCODE_WAKEUP")
    adb("shell", f"am start -S -n {PKG}/.MakepadApp "
                 f"--es makepad.SEED_L0_FILE {remote_card} "
                 f"--es makepad.SEED_L0_DATA {remote_data}")

    for _ in range(LOAD_TIMEOUT):
        time.sleep(1)
        log = adb("logcat", "-d").stdout.decode("utf-8", "replace")
        if "SEED_L0 injected" in log:
            break
        if "SEED_L0 realize failed" in log or "SEED_L0 read failed" in log:
            why = [l for l in log.splitlines() if "SEED_L0" in l][-1]
            unused = set(UNUSED.findall(why))
            if unused and source_override is None:
                src = (BASE / "cards" / f"{card_id}.card").read_text()
                return render(card_id, query, source_override=strip_sources(src, unused))
            return None, "realize: " + why[:400]
    else:
        return None, "load timeout"

    dismiss_ime()
    time.sleep(5)  # grace: live fetches land after inject; a stable skeleton is not a card
    prev, stable = grab(), 0
    for _ in range(SETTLE_TIMEOUT):
        time.sleep(1)
        cur = grab()
        stable = 0 if differs(prev, cur) else stable + 1
        prev = cur
        if stable >= 2:
            return cur, None
    return prev, None  # never settled fully; the last frame is still usable


def main():
    meta = [json.loads(l) for l in open(BASE / "meta.jsonl")]
    (BASE / "empty.json").write_text("{}")
    fail_path = BASE / "failures.jsonl"
    failed = set()
    if fail_path.exists():
        failed = {json.loads(l)["id"] for l in open(fail_path)}

    done = attempted = 0
    for m in meta:
        cid = m["id"]
        shot_path = BASE / "shots" / f"{cid}.png"
        if shot_path.exists() or cid in failed:
            done += 1
            continue
        if adb("get-state").stdout.strip() != b"device":
            print("device gone; stopping", flush=True)
            sys.exit(1)
        attempted += 1
        t0 = time.time()
        shot, err = render(cid, m['query'])
        dt = time.time() - t0
        if shot is None:
            with open(fail_path, "a") as f:
                f.write(json.dumps({"id": cid, "err": err}) + "\n")
            print(f"FAIL {cid} ({dt:.0f}s) {err}", flush=True)
        else:
            shot.save(shot_path)
            print(f"ok   {cid} ({dt:.0f}s)  [{done + attempted}/{len(meta)}]", flush=True)

    (BASE / "RENDER_DONE").write_text("done\n")
    print(f"render loop complete: {len(meta)} cards, "
          f"{len(list((BASE / 'shots').glob('*.png')))} shots, "
          f"{len(failed) + sum(1 for _ in open(fail_path)) if fail_path.exists() else 0} failures",
          flush=True)


if __name__ == "__main__":
    main()
