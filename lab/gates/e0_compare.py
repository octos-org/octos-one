#!/usr/bin/env python3
"""Is the cheap judge good enough to replace the expensive one?

Opus is the reference: 86% self-agreement, 36/36 transitive, a full ladder.
A cheaper judge has to clear three bars, and passing the first two while failing
the third is the trap — a model can be perfectly self-consistent about the wrong
thing.

  self-agreement  reads the images, not the presentation order
  transitivity    the wins collapse to one ordering
  concordance     that ordering is the SAME ordering
"""
import itertools
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent


def load(path):
    d = json.loads((HERE / path).read_text())
    n = len(d["names"])
    pairs = {}
    for r in d["results"]:
        pairs.setdefault((r["i"], r["j"]), {})[r["swap"]] = r["winner"]
    both = {k: v for k, v in pairs.items() if len(v) == 2}
    agree = sum(1 for v in both.values() if v[0] == v[1])
    wins = [0] * n
    decided = {}
    for k, v in both.items():
        if v[0] == v[1]:
            wins[v[0]] += 1
            decided[k] = v[0]
    cyc = tri = 0
    beat = {}
    for (i, j), w in decided.items():
        beat[(i, j)] = beat[(j, i)] = w
    for a, b, c in itertools.combinations(range(n), 3):
        if (a, b) in beat and (b, c) in beat and (a, c) in beat:
            tri += 1
            won = lambda x, y: beat[(x, y)] == x
            if (won(a, b) and won(b, c) and won(c, a)) or (won(b, a) and won(c, b) and won(a, c)):
                cyc += 1
    return {"names": d["names"], "n": n, "agree": agree, "pairs": len(both),
            "wins": wins, "decided": decided, "tri": tri, "cyc": cyc,
            "model": d.get("model", "opus")}


def spearman(a, b):
    n = len(a)

    def rank(v):
        o = sorted(range(n), key=lambda i: v[i])
        r = [0.0] * n
        i = 0
        while i < n:
            j = i
            while j + 1 < n and v[o[j + 1]] == v[o[i]]:
                j += 1
            for k in range(i, j + 1):
                r[o[k]] = (i + j) / 2 + 1
            i = j + 1
        return r
    ra, rb = rank(a), rank(b)
    ma, mb = sum(ra) / n, sum(rb) / n
    num = sum((x - ma) * (y - mb) for x, y in zip(ra, rb))
    den = (sum((x - ma) ** 2 for x in ra) * sum((y - mb) ** 2 for y in rb)) ** 0.5
    return num / den if den else 0.0


A = load("e0_results.json")
B = load("e0_results_zai.json")

print(f"{'':<22}{'opus':>12}{B['model']:>16}")
print(f"{'self-agreement':<22}{A['agree']}/{A['pairs']} ({100*A['agree']/A['pairs']:.0f}%)".ljust(34)
      + f"{B['agree']}/{B['pairs']} ({100*B['agree']/B['pairs']:.0f}%)".rjust(16))
print(f"{'transitive triads':<22}{A['tri']-A['cyc']}/{A['tri']}".ljust(34)
      + f"{B['tri']-B['cyc']}/{B['tri']}".rjust(16))

print(f"\n{'mockup':<26}{'opus':>6}{'glm':>6}")
for k in sorted(range(A["n"]), key=lambda i: -A["wins"][i]):
    print(f"  {A['names'][k][:24]:<24}{A['wins'][k]:>6}{B['wins'][k]:>6}")

rho = spearman(A["wins"], B["wins"])
shared = set(A["decided"]) & set(B["decided"])
same = sum(1 for k in shared if A["decided"][k] == B["decided"][k])
print(f"\nconcordance with opus")
print(f"  rank correlation      rho = {rho:+.2f}   (n=8, |rho|>0.71 is p<0.05)")
print(f"  same winner per pair  {same}/{len(shared)}  ({100*same/len(shared):.0f}%)")

print("\ndisagreements:")
for k in sorted(shared):
    if A["decided"][k] != B["decided"][k]:
        i, j = k
        print(f"  {A['names'][i][:22]:<24} vs {A['names'][j][:22]:<24}"
              f"  opus:{A['names'][A['decided'][k]][:18]:<20} glm:{A['names'][B['decided'][k]][:18]}")

verdict = ("usable as a drop-in" if rho > 0.71 and same / len(shared) > 0.8
           else "self-consistent but ranks differently — not a drop-in"
           if B["agree"] / B["pairs"] > 0.7 else "too noisy")
print(f"\nverdict: {verdict}")
