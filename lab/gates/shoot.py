#!/usr/bin/env python3
"""Render a card to a PNG on macOS, optionally under an extracted palette.

The headless backend writes PNGs but cannot draw text — its shader JIT fails to
compile the text shader (`expected function, found builtin type u32`), so every
glyph is missing and the frame is useless for judging. The device path works but
costs an APK build per change. So: run the desktop window at phone size and
capture it with `screencapture -l<windowid>`, which gives the real Metal render
including text, in about ten seconds.

Usage:
    shoot.py <card.dsl> <out.png> [palette.splash]
"""
import os
import pathlib
import subprocess
import sys
import time

APP = pathlib.Path.home() / "home/octos-one/app/target/debug/octos-app"
SIZE = os.environ.get("GATE_WINDOW", "360x780")
SETTLE = float(os.environ.get("SHOOT_SETTLE", "11"))


def window_id():
    import Quartz
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly | Quartz.kCGWindowListExcludeDesktopElements,
        Quartz.kCGNullWindowID)
    for w in wl:
        if "octos" in w.get("kCGWindowOwnerName", "").lower():
            return w["kCGWindowNumber"]
    return None


def shoot(dsl, out, palette=None, crop_bottom=90, data=None):
    """`data` switches from pre-lowered DSL to the CARD path.

    SEED_CARD_FILE pushes DSL that was already lowered, with every colour baked
    in as a literal — the kit and the palette chain never run, so a palette
    override changes nothing. SEED_L0_FILE hands the app a card and a data
    snapshot and lets it assemble the kit itself, which is the only path where
    an axis override is reachable."""
    env = dict(os.environ,
               MAKEPAD_WINDOW_SIZE=SIZE,
               MAKEPAD_DEV_GOAL_FILE="/data/local/tmp/__no_dev_loop__")
    if data:
        env["MAKEPAD_SEED_L0_FILE"] = str(dsl)
        env["MAKEPAD_SEED_L0_DATA"] = str(data)
        env.pop("MAKEPAD_SEED_CARD_FILE", None)
    else:
        env["MAKEPAD_SEED_CARD_FILE"] = str(dsl)
    if palette:
        env["MAKEPAD_L0_PALETTE_OVERRIDE"] = str(palette)
    else:
        env.pop("MAKEPAD_L0_PALETTE_OVERRIDE", None)
    p = subprocess.Popen([str(APP)], env=env,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(SETTLE)
        wid = window_id()
        if wid is None:
            return False
        r = subprocess.run(["screencapture", "-o", "-x", f"-l{wid}", str(out)],
                           capture_output=True)
        if r.returncode != 0 or not pathlib.Path(out).exists():
            return False
    finally:
        p.terminate()
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()

    # The connection toast and the composer sit below the card and are app
    # chrome, not design — judging them would compare the same strip every time.
    from PIL import Image
    im = Image.open(out)
    im.crop((0, 0, im.width, im.height - crop_bottom * im.height // 780)).save(out)
    return True


if __name__ == "__main__":
    dsl, out = sys.argv[1], sys.argv[2]
    pal = sys.argv[3] if len(sys.argv) > 3 else None
    print("ok" if shoot(pathlib.Path(dsl), pathlib.Path(out),
                        pathlib.Path(pal) if pal else None) else "FAILED")
