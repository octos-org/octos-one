#!/usr/bin/env python3
"""E1: can the mockup ranking be predicted without a model call?

E0 found a consistent gradient across 8 mockups — 86% self-agreement, 100%
transitive — and the judge's own reasons name one axis over and over:
restraint, negative space, disciplined grid, clear hierarchy.

If that axis is real, it should be computable. The metrics below are the same
Ngo / Aalto-Interface-Metrics family that failed earlier in
`../style-factory/FINDINGS-cheap-gates.md` — but they failed as DEFECT detectors
on device screenshots full of bezel and app chrome. Here they are being used for
the job they were designed for (predicting taste) on the input they assume
(a clean, segmented-by-construction image). Different question, different input.

Spearman against the judge's ranking. With n=8, |rho| > 0.71 is p < 0.05.
"""
import collections
import json
import pathlib

from PIL import Image, ImageFilter

HERE = pathlib.Path(__file__).resolve().parent
POOL = HERE / "e0"
W = 300          # everything measured at one scale, so counts are comparable


def load(p):
    im = Image.open(p).convert("RGB")
    return im.resize((W, int(im.height * W / im.width)))


def negative_space(im):
    """Share of the image holding the single dominant fill. Ngo's density,
    inverted — how much of the design is left alone."""
    q = im.quantize(colors=24).convert("RGB")
    px = list(q.getdata())
    top = collections.Counter(px).most_common(1)[0][1]
    return top / len(px)


def hue_count(im):
    """Distinct saturated hues carrying real area. AIM's colour variability."""
    hsv = im.convert("HSV")
    c = collections.Counter()
    for h, s, v in hsv.getdata():
        if s > 70 and 30 < v < 250:
            c[h // 18] += 1
    total = im.width * im.height
    return len([b for b, n in c.items() if n > total * 0.005])


def ink_bands(im):
    """Horizontal bands of content, as (top, bottom). A design's rhythm."""
    g = im.convert("L")
    px = g.load()
    bg = collections.Counter(g.getdata()).most_common(1)[0][0]
    rows, run = [], None
    for y in range(im.height):
        busy = sum(1 for x in range(0, im.width, 3) if abs(px[x, y] - bg) > 40)
        if busy > im.width * 0.02 and run is None:
            run = y
        elif busy <= im.width * 0.02 and run is not None:
            if y - run > 3:
                rows.append((run, y))
            run = None
    if run is not None:
        rows.append((run, im.height))
    return rows


def scale_spread(im):
    """Tallest band over the median band. A design with one dominant element
    has a big number; one where everything is the same weight has ~1."""
    b = ink_bands(im)
    if len(b) < 3:
        return 1.0
    hs = sorted(x[1] - x[0] for x in b)
    med = hs[len(hs) // 2]
    return hs[-1] / max(1, med)


def clutter(im):
    """Edge density — the cheapest stand-in for Rosenholtz feature congestion."""
    e = im.convert("L").filter(ImageFilter.FIND_EDGES)
    d = list(e.getdata())
    return sum(1 for v in d if v > 40) / len(d)


METRICS = {
    "neg_space": negative_space,
    "hues": hue_count,
    "scale_spread": scale_spread,
    "clutter": clutter,
}


def spearman(a, b):
    n = len(a)

    def rank(v):
        order = sorted(range(n), key=lambda i: v[i])
        r = [0.0] * n
        i = 0
        while i < n:
            j = i
            while j + 1 < n and v[order[j + 1]] == v[order[i]]:
                j += 1
            avg = (i + j) / 2 + 1
            for k in range(i, j + 1):
                r[order[k]] = avg
            i = j + 1
        return r

    ra, rb = rank(a), rank(b)
    ma, mb = sum(ra) / n, sum(rb) / n
    num = sum((x - ma) * (y - mb) for x, y in zip(ra, rb))
    den = (sum((x - ma) ** 2 for x in ra) * sum((y - mb) ** 2 for y in rb)) ** 0.5
    return num / den if den else 0.0


def main():
    d = json.loads((HERE / "e0_results.json").read_text())
    names = d["names"]
    # wins, counting only pairs the judge was consistent about
    pairs = {}
    for r in d["results"]:
        pairs.setdefault((r["i"], r["j"]), {})[r["swap"]] = r["winner"]
    wins = [0] * len(names)
    for v in pairs.values():
        if len(v) == 2 and v[0] == v[1]:
            wins[v[0]] += 1

    shots = sorted(p for p in POOL.glob("*.png") if "sheet" not in p.name)
    vals = {k: [] for k in METRICS}
    print(f"{'mockup':<26}{'wins':>5}", end="")
    for k in METRICS:
        print(f"{k:>14}", end="")
    print()
    for idx, p in enumerate(shots):
        im = load(p)
        print(f"{names[idx][:25]:<26}{wins[idx]:>5}", end="")
        for k, fn in METRICS.items():
            v = fn(im)
            vals[k].append(v)
            print(f"{v:>14.3f}", end="")
        print()

    print(f"\nSpearman vs the judge's ranking (n={len(shots)}, |rho|>0.71 is p<0.05):")
    for k in METRICS:
        rho = spearman(wins, vals[k])
        mark = "  <-- predicts" if abs(rho) > 0.71 else ""
        print(f"  {k:<14} rho = {rho:+.2f}{mark}")


if __name__ == "__main__":
    main()
