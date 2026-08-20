#!/usr/bin/env python3
"""Does the stylist draft actually PROPOSE anything different?

The stylist run starts from the card draft and trains on a SUBSET of the card
draft's own training data, so the two checkpoints could be nearly the same
function -- in which case arm C measures "lenient verify", not "taste", and the
whole stylist framing collapses. This measures it three ways:

  weights     relative L2 distance per tensor between the two checkpoints
  proposals   on held-out anchors, how often the two drafts propose a different
              token at the same slot (and how often each matches the target)
  first slot  where they first differ inside a block

Runs inside the serving image.
"""
from __future__ import annotations
import argparse, json, os, random, sys, collections

import torch

sys.path.insert(0, "/repo/src")
from dflash_torch import DFlashCfg, DFlashDraft, TargetHeads, MASK_TOKEN_ID  # noqa
from dataset import FeatureStore                                            # noqa


def load(draft_dir, ckpt, window, block, kv_fp8, device="cuda"):
    cfg = DFlashCfg.from_model_dir(draft_dir)
    cfg.block_size = block
    cfg.ctx_window = window
    cfg.kv_fp8 = kv_fp8
    m = DFlashDraft(cfg).to(device, torch.bfloat16)
    m.load_dflash_checkpoint(draft_dir)
    if ckpt:
        sd = torch.load(ckpt, map_location="cpu", weights_only=True)
        m.load_state_dict({k: v.to(torch.bfloat16) for k, v in sd["model"].items()},
                          strict=True)
    m.eval()
    return m, cfg


@torch.no_grad()
def propose(model, heads, store, name, anchors, W, B, device):
    lo = max(store.ctx_floor(name), min(anchors) - W)
    hi = max(anchors)
    h, cpos = store.context(name, lo, hi)
    h = h.to(device, torch.bfloat16); cpos = cpos.to(device)
    ids, bpos, grp, anc, labels = [], [], [], [], []
    for gi, a in enumerate(anchors):
        toks = store.tokens(name, a, a + B)
        blk = torch.full((B,), MASK_TOKEN_ID, dtype=torch.long)
        blk[0] = toks[0]
        ids.append(blk); labels.append(toks)
        bpos.append(torch.arange(a, a + B))
        grp.append(torch.full((B,), gi)); anc.append(torch.full((B,), a))
    emb = heads.embed_block(torch.cat(ids).to(device))
    out = model(h, cpos, emb, torch.cat(bpos).to(device),
                torch.cat(grp).to(device), torch.cat(anc).to(device))
    out = out.view(len(anchors), B, -1)[:, 1:, :]
    d = heads.logits(out.reshape(-1, out.shape[-1])).argmax(-1).view(len(anchors), B - 1)
    return d.cpu(), torch.stack([l[1:] for l in labels])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--feat-dir", default="/feats")
    ap.add_argument("--base", default="/models/draft")
    ap.add_argument("--ckpt-a", default=None)
    ap.add_argument("--ckpt-b", required=True)
    ap.add_argument("--target", default="/models/target")
    ap.add_argument("--block", type=int, default=16)
    ap.add_argument("--window", type=int, default=4096)
    ap.add_argument("--holdout", default="/dt/splits/holdout_all.json")
    ap.add_argument("--slices", default="/dt/splits/slices.json")
    ap.add_argument("--per-seq", type=int, default=8)
    ap.add_argument("--max-seqs", type=int, default=120)
    ap.add_argument("--out", default="/dt/stylist_divergence.json")
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()
    dev = "cuda"

    # ---- weights
    sa = (torch.load(args.ckpt_a, map_location="cpu", weights_only=True)["model"]
          if args.ckpt_a else None)
    sb = torch.load(args.ckpt_b, map_location="cpu", weights_only=True)["model"]
    if sa is None:
        from safetensors.torch import load_file
        raw = load_file(os.path.join(args.base, "model.safetensors"))
        sa = {k[:-7] if k.endswith(".weight") and (k[:-7].endswith("q_norm")
              or k[:-7].endswith("k_norm")) else k: v for k, v in raw.items()}
    wd = []
    for k in sb:
        if k in sa:
            a, b = sa[k].float(), sb[k].float()
            wd.append((k, float((b - a).norm() / max(1e-9, a.norm()))))
    wd.sort(key=lambda x: -x[1])
    tot = float(torch.cat([ (sb[k].float()-sa[k].float()).flatten() for k in sb if k in sa]).norm()
                / torch.cat([sa[k].float().flatten() for k in sb if k in sa]).norm())
    print(f"[weights] global relative L2 delta {tot:.4%}")
    print("[weights] largest per-tensor relative deltas:")
    for k, v in wd[:8]:
        print(f"    {v:8.4%}  {k}")

    # ---- proposals
    store = FeatureStore(args.feat_dir, cache_size=3)
    heads = TargetHeads.load(args.target, device=dev, embed_on_cpu=True)
    ma, _ = load(args.base, args.ckpt_a, args.window, args.block, True, dev)
    mb, _ = load(args.base, args.ckpt_b, args.window, args.block, True, dev)
    hold = set(json.load(open(args.holdout)))
    sl = json.load(open(args.slices)) if args.slices else {}
    names = [e["name"] for e in store.seqs if e["name"] in hold]
    rng = random.Random(args.seed)
    rng.shuffle(names)
    names = names[: args.max_seqs]

    agg = collections.defaultdict(lambda: collections.Counter())
    firstdiff = collections.Counter()
    for name in names:
        g0, g1 = store.gen_span(name)
        s0, s1 = store.span(name)
        g0, g1 = max(g0, s0), min(g1, s1)
        last = g1 - args.block - 1
        if last <= g0 + 4:
            continue
        anchors = sorted(rng.sample(range(g0, last), min(args.per_seq, last - g0)))
        # keep anchors in one span so they share a context tensor
        anchors = [a for a in anchors if a - anchors[0] < 1024] or anchors[:1]
        da, tgt = propose(ma, heads, store, name, anchors, args.window, args.block, dev)
        db, _ = propose(mb, heads, store, name, anchors, args.window, args.block, dev)
        lab = sl.get(name, store.by_name[name]["mode"])
        for key in ("ALL", lab):
            c = agg[key]
            c["slots"] += da.numel()
            c["diff"] += int((da != db).sum())
            c["a_correct"] += int((da == tgt).sum())
            c["b_correct"] += int((db == tgt).sum())
            c["a_only_correct"] += int(((da == tgt) & (db != tgt)).sum())
            c["b_only_correct"] += int(((db == tgt) & (da != tgt)).sum())
            c["blocks"] += da.shape[0]
            c["blocks_diff"] += int(((da != db).any(dim=1)).sum())
        for r in range(da.shape[0]):
            ne = (da[r] != db[r]).nonzero()
            firstdiff[int(ne[0]) if ne.numel() else -1] += 1

    out = {}
    print(f"\n{'slice':22s} {'slots':>8s} {'diff%':>7s} {'blk diff%':>10s} "
          f"{'A acc%':>7s} {'B acc%':>7s}")
    for k, c in sorted(agg.items()):
        e = {kk: int(vv) for kk, vv in c.items()}
        e["diff_frac"] = c["diff"] / max(1, c["slots"])
        e["blocks_diff_frac"] = c["blocks_diff"] / max(1, c["blocks"])
        e["a_acc"] = c["a_correct"] / max(1, c["slots"])
        e["b_acc"] = c["b_correct"] / max(1, c["slots"])
        out[k] = e
        print(f"{k:22s} {c['slots']:8d} {100*e['diff_frac']:7.2f} "
              f"{100*e['blocks_diff_frac']:10.2f} {100*e['a_acc']:7.2f} {100*e['b_acc']:7.2f}")
    out["_weights"] = {"global_rel_l2": tot, "per_tensor": dict(wd[:20])}
    out["_first_diff_slot"] = {str(k): v for k, v in sorted(firstdiff.items())}
    json.dump(out, open(args.out, "w"), indent=1)
    print(f"\nwrote {args.out}")


if __name__ == "__main__":
    main()
