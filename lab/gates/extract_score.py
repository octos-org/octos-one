#!/usr/bin/env python3
"""Score E1/E2/E3: how far do three independent extractors agree?

  E1 palette      colours, compared in CIE Lab as deltaE. Pixels do not have
                  opinions about their own colour, so `cv` is a real reference
                  here — a model far from it is wrong, not merely different.
  E2 type         hero-to-body ratio and distinct size count. No reference; the
                  three only bound each other.
  E3 composition  margin fraction, band count, hero alignment. Same.

deltaE76 rules of thumb: <2 imperceptible, <10 the same colour family,
>25 a different colour.
"""
import itertools
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
D = json.loads((HERE / "extract_results.json").read_text())
WHO = ["cv", "opus", "glm"]


def rgb(h):
    if not isinstance(h, str):
        return None
    h = h.strip().lstrip("#")
    if len(h) != 6:
        return None
    try:
        return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))
    except ValueError:
        return None


def lab(c):
    def f(t):
        return t ** (1 / 3) if t > 0.008856 else 7.787 * t + 16 / 116
    r, g, b = [(v / 255) for v in c]
    r, g, b = [((v + 0.055) / 1.055) ** 2.4 if v > 0.04045 else v / 12.92 for v in (r, g, b)]
    x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047
    y = (0.2126 * r + 0.7152 * g + 0.0722 * b)
    z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883
    fx, fy, fz = f(x), f(y), f(z)
    return 116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)


def dE(a, b):
    A, B = rgb(a), rgb(b)
    if A is None or B is None:
        return None
    la, lb = lab(A), lab(B)
    return sum((x - y) ** 2 for x, y in zip(la, lb)) ** 0.5


names = sorted(D)
print("E1 — PALETTE   deltaE between extractors (<10 = same colour family)\n")
for field in ("ground", "ink", "accent"):
    print(f"  {field}")
    for a, b in itertools.combinations(WHO, 2):
        ds = [dE(D[n].get(a, {}).get(field), D[n].get(b, {}).get(field)) for n in names]
        ds = [d for d in ds if d is not None]
        if not ds:
            continue
        med = sorted(ds)[len(ds) // 2]
        close = sum(1 for d in ds if d < 10)
        print(f"    {a:>4} vs {b:<5} median dE {med:5.1f}   within 10: {close}/{len(ds)}")
    print()

print("  per-mockup ground colour")
print(f"    {'mockup':<26}{'cv':>10}{'opus':>10}{'glm':>10}   dE(opus,glm)")
for n in names:
    g = [D[n].get(w, {}).get("ground", "-") for w in WHO]
    d = dE(g[1], g[2])
    print(f"    {n[:24]:<26}{g[0]:>10}{g[1]:>10}{g[2]:>10}   {d:9.1f}" if d is not None
          else f"    {n[:24]:<26}{g[0]:>10}{g[1]:>10}{g[2]:>10}         -")

print("\n\nE2 — TYPE   hero-to-body ratio\n")
print(f"  {'mockup':<26}{'cv':>8}{'opus':>8}{'glm':>8}")
for n in names:
    v = [D[n].get(w, {}).get("hero_to_body") for w in WHO]
    print(f"  {n[:24]:<26}" + "".join(f"{x:>8}" if x is not None else f"{'-':>8}" for x in v))

print("\n\nE3 — COMPOSITION   bands / margin fraction / hero alignment\n")
print(f"  {'mockup':<24}{'cv':>18}{'opus':>18}{'glm':>18}")
for n in names:
    cells = []
    for w in WHO:
        d = D[n].get(w, {})
        cells.append(f"{d.get('bands','-')}/{d.get('margin_fraction','-')}/{str(d.get('hero_align','-'))[:3]}")
    print(f"  {n[:22]:<24}" + "".join(f"{c:>18}" for c in cells))

# alignment is categorical — the only field with a clean agreement measure
print("\n  hero_align agreement")
for a, b in itertools.combinations(WHO, 2):
    same = sum(1 for n in names
               if D[n].get(a, {}).get("hero_align") == D[n].get(b, {}).get("hero_align"))
    print(f"    {a:>4} vs {b:<5} {same}/{len(names)}")
