#!/usr/bin/env python3
"""Render several margins.py outputs as one markdown table (STYLIST.md §3 form)."""
from __future__ import annotations
import argparse, json, sys

KEEP = ("rank2", "rank3", "rank5", "rank10")   # plus every non-empty tail bin


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="+", help="tag=path.json")
    ap.add_argument("--slice", default="ALL")
    args = ap.parse_args()
    tags, data = [], {}
    for f in args.files:
        tag, _, path = f.partition("=")
        tags.append(tag)
        data[tag] = json.load(open(path))["slices"][args.slice]

    def row(label, get):
        cells = " | ".join(get(data[t]) for t in tags)
        return f"| {label} | {cells} |"

    print(f"### slice `{args.slice}`\n")
    print("| | " + " | ".join(tags) + " |")
    print("|---|" + "---|" * len(tags))
    print(row("verify steps", lambda d: f"{d['steps']}"))
    print(row("rejections sampled", lambda d: f"{d['rejections']}"))
    print(row("accept / verify", lambda d: f"{d['accept_per_verify']:.2f}"))
    print(row("teacher top1 == recorded", lambda d: f"{d['teacher_top1_agrees']*100:.2f}%"))
    print("")
    print("**Rank of the draft's token in the target distribution, at the slot "
          "where exact verify first rejects it** (cumulative)\n")
    print("| | " + " | ".join(tags) + " |")
    print("|---|" + "---|" * len(tags))
    keys = list(data[tags[0]]["rank_cum"].keys())
    for k in keys:
        last = k == keys[-1]
        if k not in KEEP and not last and \
           all(data[t]["rank_share"].get(k, 0) < 0.005 for t in tags):
            continue
        lab = f"{k.replace('rank', 'rank ')}" if not last else k
        print(row(f"<= {lab}" if not last else f"{lab} (beyond the teacher's top-K)",
                  lambda d, k=k, last=last:
                  f"{(d['rank_share'] if last else d['rank_cum']).get(k,0)*100:.1f}%"))
    print("")
    print("**Logit gap, target top-1 minus the draft's token** (cumulative)\n")
    print("| | " + " | ".join(tags) + " |")
    print("|---|" + "---|" * len(tags))
    for t in ("<0.5", "<1.0", "<2.0", "<3.0", "<4.0"):
        print(row(t, lambda d, t=t: f"{d['margin_cum'][t]*100:.1f}%"))
    print(row(">= 6.0", lambda d: f"{d['margin_ge6']*100:.1f}%"))
    print(row("mean gap", lambda d: f"{d['mean_margin']:.2f}"))
    print(row("mean target prob of that token",
              lambda d: f"{d['mean_p_target_of_draft_token']:.4f}"))
    print("")
    print("**What a lenient rule would additionally accept**\n")
    print("| | " + " | ".join(tags) + " |")
    print("|---|" + "---|" * len(tags))
    for k in ("k=2", "k=3", "k=5", "k=10"):
        print(row(f"top-{k[2:]}", lambda d, k=k: f"{d['topk_would_accept'][k]*100:.1f}%"))
    for k in ("tau=1.0", "tau=2.0", "tau=3.0"):
        print(row(k, lambda d, k=k: f"{d['tau_would_accept'][k]*100:.1f}%"))


if __name__ == "__main__":
    main()
