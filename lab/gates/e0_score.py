#!/usr/bin/env python3
"""Score E0: is there a gradient, and is it the same gradient every time?

Two independent checks, because either alone can look good for the wrong reason.

  self-agreement  the same pair judged in both orders must name the same winner.
                  50% is a coin. This is the honest measure of whether the judge
                  is reading the images or the presentation order.

  transitivity    if A beats B and B beats C, A should beat C. Circular triads
                  mean each comparison found something local rather than a shared
                  quality ordering — a gradient you cannot extract from.
"""
import itertools
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
d = json.loads((HERE / "e0_results.json").read_text())
names, res = d["names"], d["results"]
n = len(names)

# --- self-agreement -------------------------------------------------------
pairs = {}
for r in res:
    pairs.setdefault((r["i"], r["j"]), {})[r["swap"]] = r["winner"]
both = {k: v for k, v in pairs.items() if len(v) == 2}
agree = sum(1 for v in both.values() if v[0] == v[1])
print(f"self-agreement under order swap:  {agree}/{len(both)}  ({100*agree/len(both):.0f}%)")
print(f"  (50% = guessing; measured 87% on rendered cards earlier)")

# --- wins, counting only pairs the judge was consistent about -------------
wins = [0] * n
solid = 0
for (i, j), v in both.items():
    if v[0] != v[1]:
        continue                      # the judge contradicted itself; no vote
    solid += 1
    wins[v[0]] += 1
print(f"\n{solid}/{len(both)} pairs decided consistently\n")

order = sorted(range(n), key=lambda k: -wins[k])
for k in order:
    print(f"  {wins[k]}/{n-1}  {names[k]}")

# --- transitivity ---------------------------------------------------------
beat = {}
for (i, j), v in both.items():
    if v[0] != v[1]:
        continue
    w = v[0]
    beat[(i, j)] = w
    beat[(j, i)] = w


def won(a, b):
    return beat.get((a, b)) == a


cyc = tri = 0
for a, b, c in itertools.combinations(range(n), 3):
    if (a, b) not in beat or (b, c) not in beat or (a, c) not in beat:
        continue
    tri += 1
    # a cycle is a >b >c >a in either rotation
    if (won(a, b) and won(b, c) and won(c, a)) or (won(b, a) and won(c, b) and won(a, c)):
        cyc += 1
print(f"\ntransitivity: {tri-cyc}/{tri} triads consistent"
      f"  ({100*(tri-cyc)/tri:.0f}%)" if tri else "\ntransitivity: no complete triads")

print(f"\nverdict: ", end="")
if agree / len(both) < 0.65:
    print("NO usable gradient — the judge is near chance on mockups.")
elif tri and cyc / tri > 0.2:
    print("differences are real but LOCAL — no single quality ordering.")
else:
    print("a consistent gradient exists; mockups are separable.")
