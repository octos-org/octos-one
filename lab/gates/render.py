#!/usr/bin/env python3
"""Render each sample and dump its laid-out geometry.

One desktop process per sample, seeded with already-lowered DSL and sized to the
phone. The window shape matters: a card laid out at the desktop default is a
different card, and layout is the thing under test.

Usage:  render.py samples/good samples/bad ...   ->  geom/<name>.json
"""
import os
import pathlib
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
APP = pathlib.Path.home() / "home" / "octos-one" / "app" / "target" / "debug" / "octos-app"
GEOM = HERE / "geom"
SIZE = os.environ.get("GATE_WINDOW", "360x780")
FRAMES = os.environ.get("GATE_FRAMES", "22")
BUDGET = float(os.environ.get("GATE_BUDGET", "30"))


def render(dsl_path, out_path):
    if out_path.exists():
        return "cached"
    env = dict(os.environ,
               MAKEPAD_SEED_CARD_FILE=str(dsl_path),
               MAKEPAD_WINDOW_SIZE=SIZE,
               MAKEPAD_DUMP_GEOMETRY=str(out_path),
               MAKEPAD_DUMP_GEOMETRY_FRAMES=FRAMES,
               # keep the run offline and quiet: no server probe, no dev loop
               MAKEPAD_DEV_GOAL_FILE="/data/local/tmp/__no_dev_loop__")
    p = subprocess.Popen([str(APP)], env=env,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    t0 = time.time()
    while time.time() - t0 < BUDGET:
        if out_path.exists():
            break
        if p.poll() is not None:
            break
        time.sleep(0.25)
    if p.poll() is None:
        p.terminate()
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
    return "ok" if out_path.exists() else "TIMEOUT"


def main():
    GEOM.mkdir(exist_ok=True)
    todo = []
    for root in sys.argv[1:]:
        for p in sorted(pathlib.Path(root).glob("*.dsl")):
            todo.append(p)
    t0 = time.time()
    bad = 0
    for i, p in enumerate(todo, 1):
        out = GEOM / (p.stem + ".json")
        r = render(p, out)
        if r == "TIMEOUT":
            bad += 1
        el = time.time() - t0
        print(f"  [{i}/{len(todo)}] {p.stem:<22} {r:<8} "
              f"{el:5.0f}s elapsed, ~{el/i*(len(todo)-i):4.0f}s left", flush=True)
    print(f"\n{len(todo)-bad}/{len(todo)} rendered in {time.time()-t0:.0f}s")


if __name__ == "__main__":
    main()
