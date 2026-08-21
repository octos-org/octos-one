#!/usr/bin/env python3
"""How soft is the target's "soft distribution", really?

This gates the whole distillation follow-up. KL(p_T || p_S) differs from
hard-label cross-entropy only by how much probability mass p_T puts OFF its own
argmax: KL = CE(p_T, p_S) - H(p_T), and if H(p_T) ~ 0 the two objectives have
almost the same gradient. So before reading anything into a KL run, measure
H(p_T) on the actual data.

CPU only; runs while the GPU serves.
"""
from __future__ import annotations
import argparse, glob, json, math, os, random

import torch


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--teach-dir", default="/teach")
    ap.add_argument("--n", type=int, default=200)
    ap.add_argument("--out", default=None)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()

    files = sorted(glob.glob(os.path.join(args.teach_dir, "*.pt")))
    random.Random(args.seed).shuffle(files)
    files = files[: args.n]
    print(f"[tstats] {len(files)} sequences")

    p1, gap, ent, mass, nsupp = [], [], [], [], []
    by_mode = {}
    for f in files:
        d = torch.load(f, map_location="cpu", weights_only=True)
        lo = d["prompt_len"] - d["pos0"]
        lp = d["top_lp"][max(0, lo):].float()
        if lp.numel() == 0:
            continue
        p = lp.exp()
        p1.append(p[:, 0])
        gap.append(lp[:, 0] - lp[:, 1])
        m = p.sum(-1)
        mass.append(m)
        # entropy of the top-K head plus the residual lumped as one symbol
        r = (1 - m).clamp_min(1e-9)
        e = -(p.clamp_min(1e-12) * lp.clamp_min(-60)).sum(-1) - r * r.log()
        ent.append(e)
        nsupp.append((p > 0.01).sum(-1).float())
        by_mode.setdefault(d["mode"], []).append(p[:, 0])

    def pct(x, qs=(0.01, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99)):
        x = torch.cat(x).float()
        return {f"p{int(q*100)}": round(float(x.quantile(q)), 4) for q in qs}

    P1 = torch.cat(p1); G = torch.cat(gap); E = torch.cat(ent)
    M = torch.cat(mass); NS = torch.cat(nsupp)
    res = {
        "n_seqs": len(files), "n_positions": int(P1.numel()),
        "top1_prob": {"mean": round(float(P1.mean()), 4), **pct(p1)},
        "top1_top2_logit_gap": {"mean": round(float(G.mean()), 3), **pct(gap)},
        "entropy_nats": {"mean": round(float(E.mean()), 4), **pct(ent)},
        "topK_mass": {"mean": round(float(M.mean()), 6), **pct(mass)},
        "n_tokens_over_1pct": {"mean": round(float(NS.mean()), 3), **pct(nsupp)},
        "share_top1_over": {f">{t}": round(float((P1 > t).float().mean()), 4)
                            for t in (0.5, 0.9, 0.99, 0.999)},
        "share_entropy_under": {f"<{t}": round(float((E < t).float().mean()), 4)
                                for t in (0.01, 0.05, 0.1, 0.3, 1.0)},
        "by_mode_top1_prob_mean": {k: round(float(torch.cat(v).mean()), 4)
                                   for k, v in sorted(by_mode.items())},
    }
    print(json.dumps(res, indent=1))
    if args.out:
        json.dump(res, open(args.out, "w"), indent=1)
        print(f"[tstats] wrote {args.out}")


if __name__ == "__main__":
    main()
