#!/usr/bin/env python3
"""Catch absurd text wrapping in a rendered card, from the screenshot alone.

A vision judge costs a model call, takes a minute, and — measured across this
whole session — is insensitive to exactly the band where real defects live. This
is deterministic, runs in under a second, and catches the failure a judge missed
entirely: a live quake card that split its hero magnitude character by character
("3" / "." / "9") and broke the word "Magnitude" into "Magnitu" / "de".

The signature is geometric. In a left-to-right script a run of text is WIDER than
it is TALL. A text block taller than it is wide has been squeezed into a column
narrower than its content, and no amount of good typography survives that.

Usage:  layout_lint.py <render.png> [--dark]
"""
import sys
from PIL import Image


def blocks(path, dark_mode=False, min_px=180):
    """Connected bands of ink, as (x0, y0, x1, y1)."""
    im = Image.open(path).convert("L")
    W, H = im.size
    px = im.load()

    def ink(x, y):
        v = px[x, y]
        return v > 150 if dark_mode else v < 120

    # rows containing ink, ignoring the status bar and bottom chrome
    rows = []
    run = None
    for y in range(90, H - 260):
        has = any(ink(x, y) for x in range(30, W - 30, 2))
        if has and run is None:
            run = y
        elif not has and run is not None:
            if y - run > 6:
                rows.append((run, y))
            run = None

    out = []
    for y0, y1 in rows:
        xs = [x for y in range(y0, y1, 2) for x in range(30, W - 30, 2) if ink(x, y)]
        if len(xs) * 4 < min_px:
            continue
        out.append((min(xs), y0, max(xs), y1))
    return out, (W, H)


def merge_column(bs, gap=14):
    """Join vertically adjacent bands that share a horizontal span — a wrapped
    paragraph is many bands stacked in one column, and must be judged whole."""
    bs = sorted(bs, key=lambda b: b[1])
    out = []
    for b in bs:
        if out:
            p = out[-1]
            overlap = min(p[2], b[2]) - max(p[0], b[0])
            width = min(p[2] - p[0], b[2] - b[0])
            if b[1] - p[3] <= gap and width > 0 and overlap > width * 0.6:
                out[-1] = (min(p[0], b[0]), p[1], max(p[2], b[2]), b[3])
                continue
        out.append(b)
    return out


def lint(path, dark_mode=False):
    bs, (W, H) = blocks(path, dark_mode)
    bs = merge_column(bs)
    findings = []
    for x0, y0, x1, y1 in bs:
        w, h = x1 - x0, y1 - y0
        if w <= 0:
            continue
        if h > w:
            findings.append((f"text block taller than wide ({w}x{h})", x0, y0, w, h))
        elif w < W * 0.18 and h > 60:
            findings.append((f"text squeezed into {100*w/W:.0f}% of the width", x0, y0, w, h))
    return findings, len(bs)


if __name__ == "__main__":
    dark = "--dark" in sys.argv
    for p in [a for a in sys.argv[1:] if not a.startswith("--")]:
        f, n = lint(p, dark)
        name = p.rsplit("/", 1)[-1]
        if f:
            print(f"  {name}: {len(f)} FAILURES of {n} text blocks")
            for msg, x, y, w, h in f:
                print(f"      at ({x},{y})  {msg}")
        else:
            print(f"  {name}: clean — {n} text blocks, none squeezed")
