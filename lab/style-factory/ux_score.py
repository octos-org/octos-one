#!/usr/bin/env python3
"""Score the four mood renders two ways.

ABSOLUTE: a 1-10 UX/aesthetic score per render, which is the goal's target.
PAIRED:   blind, order-swapped, new render vs the committed pre-change
          baseline. Only `light` carries a lift, so dark/glass/photo are a
          control — a judge that prefers the new light render while splitting
          evenly on the other three is reporting the capability, not noise.
"""
import json, re, subprocess, sys
from pathlib import Path

SP = Path("/private/tmp/claude-501/-Users-yuechen-home-Splash/10acdae2-9f20-4e60-b7b1-7195c1bdb439/scratchpad")
BASE = Path("/Users/yuechen/home/octos-one/lab/style-factory/baselines")
MOODS = ["light", "dark", "glass", "photo"]


def claude(prompt, timeout=900):
    r = subprocess.run(["claude", "-p", prompt, "--model", "opus",
                        "--allowedTools", "Read", "--output-format", "json"],
                       capture_output=True, text=True, timeout=timeout, cwd=str(SP))
    return json.loads(r.stdout)["result"]


def obj(text):
    m = re.search(r"\{.*\}", text, re.S)
    return json.loads(m.group(0))


def absolute(png):
    return obj(claude(
        f"Read {png} — a weather screen rendered live on an Android phone. "
        f"Score it as a working mobile UI a designer would ship. IGNORE the small "
        f"floating action button, any bottom app chrome, and the status bar — none "
        f"are the design's doing. JUDGE: composition and hierarchy, typographic "
        f"scale, colour relationships, spacing rhythm, shape language, surface "
        f"treatment and depth. Be a demanding critic: 9-10 means you would ship it "
        f"as a flagship app screen; 5 means competent but plain; 2 means broken. "
        f'Return ONLY JSON: {{"ux": 1-10, "why": "<=30 words", '
        f'"biggest_gap": "<=15 words"}}'))


def paired(a, b, swap):
    """a = old, b = new. Returns +1 when the NEW one wins."""
    first, second = (b, a) if swap else (a, b)
    out = claude(
        f"Read {first} then {second}. Both are the same weather screen rendered by "
        f"two builds of the same app. Ignoring the floating button, bottom chrome "
        f"and status bar, which is the better-designed screen? Weigh depth and "
        f"surface separation, hierarchy, spacing rhythm and polish. "
        f'Return ONLY JSON: {{"winner": "first"|"second", "why": "<=25 words"}}')
    d = obj(out)
    win_first = d["winner"] == "first"
    new_won = win_first if swap else not win_first
    return (1 if new_won else -1), d.get("why", "")


if __name__ == "__main__":
    print("=== ABSOLUTE UX (goal target: 9/10) ===")
    scores = {}
    for m in MOODS:
        p = SP / f"new_{m}.png"
        if not p.exists():
            print(f"  {m}: render missing"); continue
        try:
            d = absolute(str(p)); scores[m] = d["ux"]
            print(f"  {m:<6} ux={d['ux']}/10  {d['why'][:70]}")
            print(f"         gap: {d.get('biggest_gap','')[:60]}")
        except Exception as e:
            print(f"  {m}: judge failed {e}")
    if scores:
        print(f"  mean {sum(scores.values())/len(scores):.2f}")

    print("\n=== PAIRED vs pre-change baseline (light has the lift; others are control) ===")
    for i, m in enumerate(MOODS):
        old, new = BASE / f"baseline-{m}.png", SP / f"new_{m}.png"
        if not (old.exists() and new.exists()):
            print(f"  {m}: missing"); continue
        try:
            v, why = paired(str(old), str(new), swap=(i % 2 == 0))
            print(f"  {m:<6} {'NEW wins' if v > 0 else 'old wins':<9} {why[:66]}")
        except Exception as e:
            print(f"  {m}: judge failed {e}")
