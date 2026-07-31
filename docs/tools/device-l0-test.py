#!/usr/bin/env python3
"""
Render L0 cards on a real device and check what actually reached the screen.

Unit tests prove a card parses, realizes and lowers. They cannot prove it is
LEGIBLE. Three bugs in this work were invisible to 48 passing tests and obvious
in one screenshot: a forced width wrapped a hero one character per line, a tint
was passed through and ignored, and hero text clipped off the right edge.

`uiautomator` cannot help here — makepad renders to a GPU surface, so the view
hierarchy holds 14 nodes and one string. The screen is the only witness, so this
compares pixels against a golden image.

    device-l0-test.py capture <case>   # record a golden (review it by eye first)
    device-l0-test.py run [case…]      # check against goldens; exit 1 on failure

Goldens live in docs/tools/golden/. Regenerate deliberately, never to make a
failure go away — a golden updated without looking is a test that asserts
whatever the code currently does.
"""

import json
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SPLASH = ROOT.parent / "Splash"
GOLDEN = Path(__file__).parent / "golden"
ADB = Path.home() / "Library/Android/sdk/platform-tools/adb"
DEVICE = "bf0a4730"  # OnePlus 6T
PKG = "dev.makepad.octos_app"
CARDS = f"/storage/emulated/0/Android/media/{PKG}/cards"

# Status bar carries a clock and battery; the nav bar and FAB move. Comparing
# them would fail on the minute rather than on the card.
CROP_TOP = 90
CROP_BOTTOM = 180

# Cases: (name, card, data, event, payload). An event means "dispatch this
# first", so a case can assert a post-interaction state — the thing a static
# screenshot cannot reach.
CASES = [
    ("weather", "weather.card", "weather.json", None, None),
    ("stock-list", "stock.card", "stock.json", None, None),
    ("stock-detail", "stock.card", "stock.json", "open_quote", "NVDA"),
    ("news", "news.card", "news.json", None, None),
]

# Measured, not guessed: a clean re-run of all four cases differs by 0.00%, so
# rendering is deterministic once the photo backdrop is cached. 2% was the first
# guess and it was too loose — reintroducing a real regression drifted stock-list
# by 1.82% and PASSED. 0.5% leaves margin for a re-fetched backdrop while still
# failing on a layout change.
PIXEL_TOLERANCE = 24
FAIL_FRACTION = 0.005


def adb(*args, **kw):
    return subprocess.run([str(ADB), "-s", DEVICE, *args], capture_output=True, **kw)


def lower(case):
    """Realize a card (optionally after one event) and return the DSL."""
    _, card, data, event, payload = case
    card_path = SPLASH / "crates/splash-core/tests/fixtures" / card
    data_path = Path(__file__).parent / "data" / data

    if event:
        args = ["--example", "tap_l0", "--", str(card_path), str(data_path), event]
        if payload:
            args.append(payload)
    else:
        args = ["--example", "lower_l0", "--", str(card_path), str(data_path)]

    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "splash-core", *args],
        cwd=SPLASH, capture_output=True, text=True,
    )
    if out.returncode != 0:
        raise SystemExit(f"lowering {case[0]} failed:\n{out.stderr}")
    return out.stdout


def render(name, dsl):
    """Push, launch, settle, screenshot. Returns the cropped card region."""
    local = Path(f"/tmp/l0-{name}.splash")
    local.write_text(dsl)
    adb("push", str(local), f"{CARDS}/l0-{name}.splash")
    adb("shell", "am", "force-stop", PKG)
    adb("shell", "input", "keyevent", "KEYCODE_WAKEUP")
    adb("shell", f"am start -n {PKG}/.MakepadApp "
                 f"--es makepad.SEED_CARD_FILE {CARDS}/l0-{name}.splash")
    # The photo backdrop is fetched over the network; settle before capturing.
    subprocess.run(["sleep", "13"])
    shot = adb("exec-out", "screencap", "-p").stdout
    path = Path(f"/tmp/shot-{name}.png")
    path.write_bytes(shot)
    img = Image.open(path).convert("RGB")
    return img.crop((0, CROP_TOP, img.width, img.height - CROP_BOTTOM))


def compare(a, b):
    """Fraction of pixels differing beyond tolerance on any channel."""
    if a.size != b.size:
        return 1.0
    d = np.abs(np.asarray(a, dtype=np.int16) - np.asarray(b, dtype=np.int16))
    return float((d.max(axis=2) > PIXEL_TOLERANCE).mean())


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "run"
    wanted = sys.argv[2:]
    cases = [c for c in CASES if not wanted or c[0] in wanted]
    GOLDEN.mkdir(exist_ok=True)

    if adb("get-state").stdout.strip() != b"device":
        raise SystemExit(f"device {DEVICE} is not attached")

    failures = []
    for case in cases:
        name = case[0]
        shot = render(name, lower(case))
        gold_path = GOLDEN / f"{name}.png"

        if mode == "capture":
            shot.save(gold_path)
            print(f"  captured {name} -> {gold_path.name} (review it by eye)")
            continue

        if not gold_path.exists():
            failures.append(f"{name}: no golden; run `capture` and review it")
            continue

        drift = compare(shot, Image.open(gold_path).convert("RGB"))
        ok = drift <= FAIL_FRACTION
        print(f"  {'ok  ' if ok else 'FAIL'} {name:<14} {drift * 100:5.2f}% pixels differ")
        if not ok:
            diff_path = Path(f"/tmp/diff-{name}.png")
            shot.save(diff_path)
            failures.append(f"{name}: {drift*100:.2f}% drift, actual at {diff_path}")

    if failures:
        print("\n" + "\n".join(f"  {f}" for f in failures))
        return 1
    if mode == "run":
        print(f"\nall {len(cases)} cards render as expected on {DEVICE}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
