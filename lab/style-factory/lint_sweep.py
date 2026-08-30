#!/usr/bin/env python3
"""Run the squeezed-text gate over every render on disk, auto-detecting polarity.

Codex's point stands: six clean specimens bound the false-positive rate only at
39% (95% one-sided). This is the widest sweep the renders on disk allow.
"""
import sys, glob
from PIL import Image
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from layout_lint import lint

roots = sys.argv[1:] or ["."]
paths = sorted({p for r in roots for p in glob.glob(f"{r}/**/*.png", recursive=True)})
fails, clean, skipped = [], 0, 0
for p in paths:
    try:
        im = Image.open(p).convert("L")
        if im.size[0] < 300 or im.size[1] < 600:
            skipped += 1; continue
        px = im.resize((60, 120)).getdata()
        dark = (sorted(px)[len(px)//2] < 110)
        f, n = lint(p, dark)
        if n < 4:                       # nothing segmented — no verdict to give
            skipped += 1; continue
        if f:
            fails.append((p, n, f))
        else:
            clean += 1
    except Exception as e:
        skipped += 1

print(f"renders judged: {clean + len(fails)}   clean: {clean}   flagged: {len(fails)}   skipped(no segmentation/too small): {skipped}")
for p, n, f in fails:
    print(f"\n  {p.split('/')[-2]}/{p.split('/')[-1]}  ({n} blocks)")
    for msg, x, y, w, h in f[:3]:
        print(f"      ({x},{y})  {msg}")
