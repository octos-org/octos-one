#!/usr/bin/env python3
"""Choose k and tau from measured data instead of guessing.

Reads the probe stats the patched worker writes while running EXACT-equivalent
verify (mode "tau", tau 0.0, stats on). At every step it records, for the slot
where exact verify first rejected the draft:
  rank_hist    the rank the draft's token held in the target's distribution
  margin_hist  the logit gap between the target's top-1 and the draft's token
plus top2_hist, the top1-top2 gap at every slot (the natural scale for tau).
"""
import json, sys, os

BINS = ["rank2", "rank3", "rank4", "rank5", "rank6", "rank7", "rank8", "rank9",
        "rank10", "rank11-100", "rank101-1000", "rank>1000"]


def show_hist(h, width=0.25, label=""):
    tot = sum(h) or 1
    cum = 0
    print(f"  {label} (n={tot})")
    for i, v in enumerate(h):
        if v == 0:
            continue
        cum += v
        lo = i * width
        print(f"    [{lo:4.2f},{lo+width:4.2f}) {v:6d}  {100*v/tot:5.1f}%  "
              f"cum {100*cum/tot:5.1f}%")


def main():
    p = sys.argv[1] if len(sys.argv) > 1 else "/mnt/stylist-ctrl/stats_probe.json"
    a = json.load(open(p))
    print(f"[pick] {p}")
    print(f"  steps {a['steps']}  slots accepted {a['slots']}  exact {a['exact']}  "
          f"rejections sampled {a['rejections']}")
    rh = a["rank_hist"][:12]
    tot = sum(rh) or 1
    print("\n  rank of the draft token at the first exact rejection:")
    cum = 0
    for name, v in zip(BINS, rh):
        cum += v
        print(f"    {name:14s} {v:6d}  {100*v/tot:5.1f}%  cum {100*cum/tot:5.1f}%")
    print("\n  => top-k would additionally accept that slot for:")
    for k in (2, 3, 5, 10):
        n = sum(rh[: k - 1])
        print(f"       k={k:2d}  {100*n/tot:5.1f}% of rejections")
    print()
    show_hist(a["margin_hist"], label="logit gap top1 - draft token at rejection")
    print()
    show_hist(a["top2_hist"], label="logit gap top1 - top2 at every slot")
    mh = a["margin_hist"]
    mt = sum(mh) or 1
    print("\n  => tau would additionally accept that slot for:")
    for tau in (0.25, 0.5, 1.0, 1.5, 2.0, 3.0):
        n = sum(mh[: int(tau / 0.25)])
        print(f"       tau={tau:4.2f}  {100*n/mt:5.1f}% of rejections")


if __name__ == "__main__":
    main()
