#!/usr/bin/env python3
"""
Tap an L0 card on a real device and check the tap did what the profile says.

Everything about L0 has so far been verified against itself. The card parses,
realizes, lowers, and four goldens render on the 6T — and none of that proves a
tap works. `ui-profile-l0.md` §5.10.1 argues about instance identity crossing a
layer boundary; `scoped-state.md` §10 names "one card, one toggle, on device" as
the thing that would prove the contract wrong. This runs that.

THE ORACLE IS THE POINT. `device-l0-test.py` already has a `stock-detail` case:
it dispatches `open_quote` with `NVDA` ON THE MAC, lowers the result, and renders
it. That golden is what the detail view is SUPPOSED to look like.

So this seeds the LEDGER instead of a lowered card, taps the NVDA row on glass,
and compares the resulting screen to that same golden. Agreement means the
device's own dispatch — its store, its identity, its re-realization — produced
what the reference implementation produces. That is a much stronger claim than
"something changed when I tapped", which a broken card also satisfies.

    device-l0-tap.py            # seed, tap, compare against the stock-detail golden

Exit 1 on any failure.
"""

import importlib.util
import subprocess
import sys
from pathlib import Path

# Reuse the harness's device plumbing rather than reimplementing it: the
# thresholds, crop and load-marker waits in there are all measured, and a second
# copy would drift from them silently.
sys.path.insert(0, str(Path(__file__).parent))
_spec = importlib.util.spec_from_file_location(
    "l0harness", Path(__file__).parent / "device-l0-test.py"
)
H = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(H)


def open_golden(path):
    """Goldens are stored already cropped (see `capture`), so this only needs
    the colour-space conversion `compare` assumes."""
    from PIL import Image
    return Image.open(path).convert("RGB")


def wait_for(marker, timeout=H.LOAD_TIMEOUT):
    """Poll logcat for a marker. Returns the log if seen, else None."""
    for _ in range(timeout):
        subprocess.run(["sleep", "1"])
        log = H.adb("logcat", "-d").stdout.decode("utf-8", "replace")
        if marker in log:
            return log
    return None


def settle():
    """Three identical frames, as the render harness does — an intermediate
    state can hold still for one second."""
    previous = H.grab()
    stable = 0
    for _ in range(H.SETTLE_TIMEOUT):
        subprocess.run(["sleep", "1"])
        current = H.grab()
        stable = stable + 1 if H.compare(previous, current) == 0.0 else 0
        previous = current
        if stable >= 2:
            return current
    return previous


def seed_ledger(card, data):
    """Push the L0 ledger and its data, launch against them, wait for load."""
    remote_card = f"{H.CARDS}/tap.card"
    remote_data = f"{H.CARDS}/tap.json"
    H.adb("push", str(H.SPLASH / "crates/splash-ui-l0/tests/fixtures" / card), remote_card)
    H.adb("push", str(Path(__file__).parent / "data" / data), remote_data)

    H.adb("shell", "am", "force-stop", H.PKG)
    H.adb("logcat", "-c")
    H.adb("shell", "input", "keyevent", "KEYCODE_WAKEUP")
    H.adb("shell", f"am start -n {H.PKG}/.MakepadApp "
                   f"--es makepad.SEED_L0_FILE {remote_card} "
                   f"--es makepad.SEED_L0_DATA {remote_data}")

    log = wait_for("SEED_L0 injected")
    if log is None:
        print("  the app never reported seeding the ledger.")
        print("  Most likely: this APK predates SEED_L0_FILE. Rebuild and reinstall.")
        return None
    H.dismiss_ime()
    return settle()


def main():
    print("seeding the stock ledger on", H.DEVICE)
    before = seed_ledger("stock.card", "stock.json")
    if before is None:
        return 1

    # The list must be up before tapping, or the tap lands on whatever is.
    list_golden = H.GOLDEN / "stock-list.png"
    if list_golden.exists():
        drift = H.compare(open_golden(list_golden), before)
        print(f"  list view: {drift * 100:.2f}% from the stock-list golden")
        if drift > H.FAIL_FRACTION:
            print("  the seeded ledger did not render the list; not tapping.")
            before.save("/tmp/l0-tap-before.png")
            return 1

    # Tap the NVDA row. The coordinate is READ OFF the stock-list golden rather
    # than guessed: the row's "$184.20 +1.7%" line sits at y≈474 in that image,
    # and the golden is cropped by CROP_TOP, so the screen coordinate is
    # 474 + CROP_TOP. A tap that misses lands on the panel background and fires
    # nothing, which the log check below distinguishes from a tap that was
    # refused — those two failures have completely different causes.
    tap_y = 474 + H.CROP_TOP
    print(f"  tapping the NVDA row at (540, {tap_y})")
    H.adb("logcat", "-c")
    H.adb("shell", "input", "tap", "540", str(tap_y))

    log = wait_for("[l0]", timeout=10)
    if log is None:
        print("  FAIL: the tap produced no [l0] log line at all.")
        print("  The hit target never fired -- the overlay button is not receiving.")
        return 1
    line = next((l for l in log.splitlines() if "[l0]" in l), "")
    print(f"  device says: {line.strip()[-110:]}")
    if "applied to nothing" in line or "failed" in line:
        print("  FAIL: the event reached the host but did not apply.")
        return 1

    after = settle()
    after.save("/tmp/l0-tap-after.png")

    # The oracle: the Mac-side dispatch of the same event, already recorded.
    golden = H.GOLDEN / "stock-detail.png"
    if not golden.exists():
        print(f"  no golden at {golden}; wrote /tmp/l0-tap-after.png for review")
        return 1
    drift = H.compare(open_golden(golden), after)
    print(f"  detail view: {drift * 100:.2f}% from the stock-detail golden")
    if drift > H.FAIL_FRACTION:
        print("  FAIL: the tap changed the screen, but not to what dispatch produces.")
        print("  Compare /tmp/l0-tap-after.png against docs/tools/golden/stock-detail.png")
        return 1

    print("\n  a tap on device produced the same screen as the reference dispatch.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
