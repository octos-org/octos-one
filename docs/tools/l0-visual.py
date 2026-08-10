#!/usr/bin/env python3
"""Capture each card through the L0 path and diff it against the old rendering.

The goldens in `golden/` were produced by `makepad::lower` — the path this
replaces. They are therefore not a pass/fail bar any more: the theme legitimately
moved. What they are is the best available reference for "what this card looked
like when it was good", so the diff is a WORKLIST, not a verdict.

    l0-visual.py            capture all three and report drift
    l0-visual.py weather    just one
"""
import importlib.util, subprocess, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
_spec = importlib.util.spec_from_file_location("h", Path(__file__).parent / "device-l0-test.py")
H = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(H)

FIXTURES = H.SPLASH / "crates/splash-ui-l0/tests/fixtures"
CASES = [("weather", None, None), ("news", None, None),
         ("stock", None, None), ("stock-detail", "open_quote", "NVDA")]


def seed(card, data, event=None, payload=None):
    """Push the ledger, launch against it, wait for the load marker, settle."""
    rc, rd = f"{H.CARDS}/v.card", f"{H.CARDS}/v.json"
    H.adb("push", str(FIXTURES / f"{card}.card"), rc)
    H.adb("push", str(Path(__file__).parent / "data" / f"{data}.json"), rd)
    H.adb("shell", "am", "force-stop", H.PKG)
    H.adb("logcat", "-c")
    H.adb("shell", "input", "keyevent", "KEYCODE_WAKEUP")
    extra = ""
    if event:
        extra = f" --es makepad.SEED_L0_EVENT {event} --es makepad.SEED_L0_VALUE {payload}"
    H.adb("shell", f"am start -S -n {H.PKG}/.MakepadApp "
                   f"--es makepad.SEED_L0_FILE {rc} --es makepad.SEED_L0_DATA {rd}{extra}")
    for _ in range(H.LOAD_TIMEOUT):
        subprocess.run(["sleep", "1"])
        if "SEED_L0 injected" in H.adb("logcat", "-d").stdout.decode("utf-8", "replace"):
            break
    else:
        return None
    H.dismiss_ime()
    prev, stable = H.grab(), 0
    for _ in range(H.SETTLE_TIMEOUT):
        subprocess.run(["sleep", "1"])
        cur = H.grab()
        stable = stable + 1 if H.compare(prev, cur) == 0.0 else 0
        prev = cur
        if stable >= 2:
            return cur
    return prev


def main():
    want = sys.argv[1:]
    out = Path("/tmp/l0-visual")
    out.mkdir(exist_ok=True)
    for name, event, payload in CASES:
        if want and name not in want:
            continue
        card = "stock" if name.startswith("stock") else name
        data = card
        shot = seed(card, data, event, payload)
        if shot is None:
            print(f"  {name:<14} did not load")
            continue
        shot.save(out / f"{name}.png")
        gold = H.GOLDEN / f"{name}.png"
        if gold.exists():
            from PIL import Image
            drift = H.compare(Image.open(gold).convert("RGB"), shot)
            print(f"  {name:<14} {drift*100:5.1f}% from the old rendering  -> {out/name}.png")
        else:
            print(f"  {name:<14} captured (no reference)      -> {out/name}.png")


if __name__ == "__main__":
    sys.exit(main())
