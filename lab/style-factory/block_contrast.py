#!/usr/bin/env python3
"""Contrast, measured per segmented text block instead of per screen.

The whole-screen version reported 1.0:1 on a good render — it sampled app chrome
against itself. Segment first (layout_lint's row bands), then measure each
block's ink against the ring just outside it. Blocks under 12px tall are the
bezel seam and carry no ink to judge.

Usage:  block_contrast.py <render.png>[:dark] ...

"""
import sys
from collections import Counter
from PIL import Image

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from layout_lint import blocks, merge_column


def _lin(c):
    c /= 255
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

def lum(p):
    return 0.2126*_lin(p[0]) + 0.7152*_lin(p[1]) + 0.0722*_lin(p[2])

def ratio(a, b):
    la, lb = sorted((lum(a), lum(b)))
    return (lb + 0.05) / (la + 0.05)


def probe(path, dark):
    bs, (W, H) = blocks(path, dark)
    bs = merge_column(bs)
    im = Image.open(path).convert("RGB")
    px = im.load()

    # --- CONTRAST, per block: ink inside vs the ring just outside it ---
    worst = None
    for x0, y0, x1, y1 in bs:
        if y1 - y0 < 12:      # hairlines and the bezel seam carry no ink to judge
            continue
        g = Image.open(path).convert("L").load()
        ink = [px[x, y] for y in range(y0, y1, 2) for x in range(x0, x1, 2)
               if (g[x, y] > 150 if dark else g[x, y] < 120)]
        if len(ink) < 40:
            continue
        ring = []
        for d in (6, 10, 14):
            for x in range(max(0, x0-d), min(W, x1+d), 3):
                for y in (max(0, y0-d), min(H-1, y1+d)):
                    ring.append(px[x, y])
        if not ring:
            continue
        ic = Counter(ink).most_common(1)[0][0]
        bc = Counter(ring).most_common(1)[0][0]
        r = ratio(ic, bc)
        if worst is None or r < worst[0]:
            worst = (r, (x0, y0, x1-x0, y1-y0), ic, bc)

    # --- ALIGNMENT, per block: distinct left edges among BLOCKS, not scanlines ---
    lefts = sorted(b[0] for b in bs if b[3]-b[1] >= 12)
    clusters = []
    for e in lefts:
        if clusters and e - clusters[-1][-1] <= 12:
            clusters[-1].append(e)
        else:
            clusters.append([e])
    return worst, len(clusters), [c[0] for c in clusters], len(bs)


for spec in sys.argv[1:]:
    path, dark = (spec.split(":") + ["light"])[:2]
    dark = dark == "dark"
    w, ncl, edges, nb = probe(path, dark)
    name = path.rsplit("/", 1)[-1]
    if w:
        r, box, ic, bc = w
        print(f"{name:26} blocks {nb:3}  worst-contrast {r:5.1f}:1 at {box}  ink{ic} bg{bc}")
    else:
        print(f"{name:26} blocks {nb:3}  worst-contrast  n/a")
    print(f"{'':26} left-edge clusters {ncl}  {edges[:8]}")
