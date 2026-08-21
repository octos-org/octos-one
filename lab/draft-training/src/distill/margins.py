#!/usr/bin/env python3
"""Reproduce STYLIST.md section 3 -- the rejection-margin table -- OFFLINE, for
any draft checkpoint.

Section 3 was measured on a research server by instrumenting the lenient-verify
worker. That costs a production window per draft and can only be done one draft
per launch. It can be done offline instead, exactly, because under EXACT verify
the committed trajectory IS the recorded temp-0 generation: nothing the draft
proposes survives, so replaying the recorded sequence reproduces the serving
trajectory token for token. The target's distribution along that trajectory is
what src/distill/extract_logprobs.py stored.

So for every verify step we can ask the same question the server probe asked:
at the slot where exact verify FIRST rejects the draft, what rank did the
draft's token hold in the target's distribution, and how far below the target's
top-1 was its logit?

The one thing offline replay cannot resolve is rank beyond the teacher's K
(64): those land in a ">K" bucket, with a lower bound on their logit gap
(top1 - the K-th entry) which is reported so the ">= 6.0" share stays honest.

  python3 margins.py --draft <dir> [--ckpt x.pt] --teach-dir /teach \\
      --holdout splits/holdout_all.json --slices splits/slices.json --out m.json
"""
from __future__ import annotations
import argparse, collections, json, math, os, random, sys, time

import torch

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, HERE)
from dflash_torch import DFlashCfg, DFlashDraft, TargetHeads, MASK_TOKEN_ID  # noqa
from dataset import FeatureStore  # noqa
from teacher import TeacherStore  # noqa
from train_dflash import kl_term  # noqa

NBIN, BINW = 24, 0.25          # same binning as the server probe: [0,6) by 0.25


def new_acc():
    return {"steps": 0, "slots_accepted": 0, "rejections": 0,
            "rank_hist": [0] * 14, "margin_hist": [0] * NBIN,
            "top2_hist": [0] * NBIN, "beyond_k": 0, "beyond_k_unresolved": 0,
            "margin_sum": 0.0, "margin_n": 0,
            "p_draft_sum": 0.0,          # target prob of the draft's token
            "p_draft_n": 0,
            "teacher_top1_agrees": 0, "teacher_slots": 0,
            "kl_sum": 0.0, "kl_n": 0}


def bin_of(v):
    return int(min(NBIN - 1, max(0, v / BINW)))


@torch.no_grad()
def run_sequence(model, heads, store, teach, name, starts, B, W, device, acc,
                 max_steps=64):
    """Chained exact-verify replay, several independent chains sharing forwards
    (identical to eval_accept.run_sequence; it also queries the teacher)."""
    g0, g1 = store.gen_span(name)
    s0, s1 = store.span(name)
    g1 = min(g1, s1)
    chains = [{"a": a, "steps": 0} for a in starts if a + B < g1]
    if not chains:
        return
    floor = store.ctx_floor(name)
    while True:
        active = [c for c in chains if c["a"] + B < g1 and c["steps"] < max_steps]
        if not active:
            break
        anchors = [c["a"] for c in active]
        lo = max(floor, min(anchors) - W)
        hi = max(anchors)
        h, cpos = store.context(name, lo, hi)
        h = h.to(device, torch.bfloat16, non_blocking=True)
        cpos = cpos.to(device, non_blocking=True)
        ids, bpos, grp, anch, labels = [], [], [], [], []
        for gi, c in enumerate(active):
            a = c["a"]
            toks = store.tokens(name, a, a + B)
            blk = torch.full((B,), MASK_TOKEN_ID, dtype=torch.long)
            blk[0] = toks[0]
            ids.append(blk); labels.append(toks)
            bpos.append(torch.arange(a, a + B))
            grp.append(torch.full((B,), gi)); anch.append(torch.full((B,), a))
        out = model(h, cpos, heads.embed_block(torch.cat(ids).to(device)),
                    torch.cat(bpos).to(device), torch.cat(grp).to(device),
                    torch.cat(anch).to(device))
        out = out.view(len(active), B, -1)[:, 1:, :]
        lg = heads.logits(out.reshape(-1, out.shape[-1])).float()
        drafts = lg.argmax(-1).view(len(active), B - 1).cpu()

        # teacher rows for every slot of every active block, in one gather
        pos = torch.cat([torch.arange(c["a"] + 1, c["a"] + B) for c in active])
        # the control the whole read-out depends on: did the distillation
        # objective actually get optimised? Mean held-out KL(target||draft) at
        # EVERY slot, not just rejections.
        gi_, gl_, go_ = teach.gather(name, pos, device=lg.device)
        if bool(go_.any()):
            kv = kl_term(lg[go_], gi_[go_], gl_[go_])
            acc["kl_sum"] += float(kv.sum())
            acc["kl_n"] += int(go_.sum())
        t_ids, t_lp, t_ok = teach.gather(name, pos, device="cpu")
        t_ids = t_ids.view(len(active), B - 1, -1)
        t_lp = t_lp.view(len(active), B - 1, -1)
        t_ok = t_ok.view(len(active), B - 1)

        for gi, c in enumerate(active):
            tgt = labels[gi][1:]
            d = drafts[gi]
            eq = (d == tgt)
            a_len = int((~eq).nonzero()[0].item()) if (~eq).any() else int(B - 1)
            acc["steps"] += 1
            acc["slots_accepted"] += a_len
            # the target's own top1-top2 gap at every slot -- the scale for tau
            for j in range(B - 1):
                if not bool(t_ok[gi, j]):
                    continue
                lp = t_lp[gi, j]
                acc["teacher_slots"] += 1
                if int(t_ids[gi, j, 0]) == int(tgt[j]):
                    acc["teacher_top1_agrees"] += 1
                acc["top2_hist"][bin_of(float(lp[0] - lp[1]))] += 1
            if a_len < B - 1:                       # a real rejection happened
                j = a_len
                if bool(t_ok[gi, j]):
                    acc["rejections"] += 1
                    lp = t_lp[gi, j]
                    ids_j = t_ids[gi, j]
                    # -1e30 marks an unfilled slot; never let a padded id (0)
                    # masquerade as a hit
                    filled = lp > -1e29
                    lp = lp[filled]
                    ids_j = ids_j[filled]
                    hit = (ids_j == int(d[j])).nonzero()
                    if hit.numel():
                        r = int(hit[0].item()) + 1
                        gap = float(lp[0] - lp[r - 1])
                        acc["p_draft_sum"] += math.exp(float(lp[r - 1]))
                        acc["p_draft_n"] += 1
                        acc["margin_sum"] += gap
                        acc["margin_n"] += 1
                        acc["margin_hist"][bin_of(gap)] += 1
                        if r <= 10:
                            acc["rank_hist"][max(0, r - 2)] += 1
                        elif r <= 100:
                            acc["rank_hist"][9] += 1
                        elif r <= 1000:
                            acc["rank_hist"][10] += 1
                        else:
                            acc["rank_hist"][11] += 1
                    else:
                        acc["beyond_k"] += 1
                        acc["rank_hist"][12] += 1      # ">K, rank unresolved"
                        bound = float(lp[0] - lp[-1])  # gap is AT LEAST this
                        acc["p_draft_sum"] += math.exp(float(lp[-1]))
                        acc["p_draft_n"] += 1
                        if bound < 6.0:
                            acc["beyond_k_unresolved"] += 1
                        else:
                            acc["margin_hist"][NBIN - 1] += 1
                            acc["margin_n"] += 1
                            acc["margin_sum"] += bound
            c["a"] += a_len + 1
            c["steps"] += 1


def report(acc, K):
    rh, tot = acc["rank_hist"], max(1, sum(acc["rank_hist"]))
    # offline, ranks can only be resolved inside the teacher's top-K, so bins
    # 10/11 (101-1000, >1000) are structurally empty and everything past K
    # lands in the ">K" bin instead. Label them for what they actually are.
    names = ["rank2", "rank3", "rank4", "rank5", "rank6", "rank7", "rank8",
             "rank9", "rank10", f"rank11-{K}", "rank101-1000", "rank>1000",
             f"rank>{K}", "-"]
    out = {"steps": acc["steps"], "rejections": acc["rejections"],
           "accept_per_verify": acc["slots_accepted"] / max(1, acc["steps"]),
           "teacher_top1_agrees": acc["teacher_top1_agrees"] / max(1, acc["teacher_slots"]),
           "mean_margin": acc["margin_sum"] / max(1, acc["margin_n"]),
           "mean_heldout_kl": acc["kl_sum"] / max(1, acc["kl_n"]),
           "kl_slots": acc["kl_n"],
           "mean_p_target_of_draft_token": acc["p_draft_sum"] / max(1, acc["p_draft_n"]),
           "beyond_k": acc["beyond_k"], "beyond_k_unresolved": acc["beyond_k_unresolved"],
           "rank_share": {}, "rank_cum": {}}
    cum = 0
    for nm, v in zip(names, rh):
        if nm == "-":
            continue
        cum += v
        out["rank_share"][nm] = v / tot
        out["rank_cum"][nm] = cum / tot
    mh, mt = acc["margin_hist"], max(1, sum(acc["margin_hist"]))
    out["margin_cum"] = {f"<{t}": sum(mh[: int(t / BINW)]) / mt
                         for t in (0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0)}
    out["margin_ge6"] = mh[NBIN - 1] / mt
    th, tt = acc["top2_hist"], max(1, sum(acc["top2_hist"]))
    out["target_top1_top2_cum"] = {f"<{t}": sum(th[: int(t / BINW)]) / tt
                                   for t in (0.5, 1.0, 2.0, 3.0, 5.0)}
    out["margin_hist"] = mh
    out["top2_hist"] = th
    out["rank_hist"] = rh
    # what top-k / tau would additionally accept, straight from the histograms
    out["topk_would_accept"] = {f"k={k}": sum(rh[: k - 1]) / tot for k in (2, 3, 5, 10)}
    out["tau_would_accept"] = {f"tau={t}": sum(mh[: int(t / BINW)]) / mt
                               for t in (0.5, 1.0, 2.0, 3.0)}
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--feat-dir", default="/feats")
    ap.add_argument("--teach-dir", default="/teach")
    ap.add_argument("--draft", default="/models/draft")
    ap.add_argument("--ckpt", default=None)
    ap.add_argument("--target", default="/models/target")
    ap.add_argument("--block-size", type=int, default=16)
    ap.add_argument("--window", type=int, default=4096)
    ap.add_argument("--kv-fp8", action="store_true")
    ap.add_argument("--holdout", default=None)
    ap.add_argument("--slices", default=None)
    ap.add_argument("--per-seq", type=int, default=4)
    ap.add_argument("--max-seqs", type=int, default=0)
    ap.add_argument("--max-steps", type=int, default=64)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--tag", default="run")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    device = "cuda"

    cfg = DFlashCfg.from_model_dir(args.draft)
    cfg.ctx_window = args.window
    cfg.kv_fp8 = args.kv_fp8
    model = DFlashDraft(cfg).to(device, torch.bfloat16).eval()
    n, unused = model.load_dflash_checkpoint(args.draft)
    print(f"[margins] loaded {n} draft params from {args.draft}")
    if args.ckpt:
        sd = torch.load(args.ckpt, map_location="cpu", weights_only=True)
        sd = sd.get("model", sd)
        model.load_state_dict({k: v.to(torch.bfloat16) for k, v in sd.items()},
                              strict=True)
        print(f"[margins] overrode weights from {args.ckpt}")
    heads = TargetHeads.load(args.target, device=device)
    store = FeatureStore(args.feat_dir, cache_size=2)
    teach = TeacherStore(args.teach_dir, cache_size=2)

    names = [e["name"] for e in store.seqs]
    if args.holdout:
        keep = set(json.load(open(args.holdout)))
        names = [x for x in names if x in keep]
    names = [x for x in names if x in teach]
    rng = random.Random(args.seed)
    rng.shuffle(names)
    if args.max_seqs:
        names = names[: args.max_seqs]
    slice_map = json.load(open(args.slices)) if args.slices else {}
    print(f"[margins] {len(names)} sequences with both features and teacher data")

    accs = collections.defaultdict(new_acc)
    t0 = time.time()
    for i, name in enumerate(names):
        e = store.by_name[name]
        g0, g1 = store.gen_span(name)
        s0, s1 = store.span(name)
        g1 = min(g1, s1)
        room = g1 - g0 - args.block_size * 4
        if room <= 0:
            continue
        starts = sorted(rng.sample(range(g0, g0 + room),
                                   min(args.per_seq, room)))
        local = new_acc()
        run_sequence(model, heads, store, teach, name, starts, args.block_size,
                     args.window, device, local, max_steps=args.max_steps)
        keys = ["ALL", e["mode"]]
        lab = slice_map.get(name)
        if lab:
            keys.append(f"GATE:{lab}")
        for k in keys:
            a = accs[k]
            for f, v in local.items():
                if isinstance(v, list):
                    for j, x in enumerate(v):
                        a[f][j] += x
                else:
                    a[f] += v
        if (i + 1) % 25 == 0:
            print(f"  {i+1}/{len(names)} {time.time()-t0:.0f}s "
                  f"rej={accs['ALL']['rejections']}", flush=True)

    K = teach.k or 64
    res = {"config": vars(args), "K": K,
           "slices": {k: report(v, K) for k, v in sorted(accs.items())}}
    a = res["slices"]["ALL"]
    print(f"\n=== {args.tag} :: ALL ===")
    print(f"  verify steps {a['steps']}, rejections {a['rejections']}, "
          f"accept/verify {a['accept_per_verify']:.2f}")
    print(f"  teacher top1 == recorded token: {a['teacher_top1_agrees']*100:.2f}%")
    print("  rank of the draft token at the first exact rejection:")
    for k, v in a["rank_share"].items():
        if v:
            print(f"    {k:22s} {v*100:5.1f}%   cum {a['rank_cum'][k]*100:5.1f}%")
    print(f"  mean held-out KL(target || draft) over {a['kl_slots']} slots: "
          f"{a['mean_heldout_kl']:.4f} nats")
    print(f"  mean target prob of the draft's rejected token: "
          f"{a['mean_p_target_of_draft_token']:.4f}")
    print("  logit gap top1 - draft token:")
    for k, v in a["margin_cum"].items():
        print(f"    {k:8s} {v*100:5.1f}%")
    print(f"    >=6.0    {a['margin_ge6']*100:5.1f}%")
    print("  target's own top1-top2 gap at every slot:")
    for k, v in a["target_top1_top2_cum"].items():
        print(f"    {k:8s} {v*100:5.1f}%")
    if args.out:
        json.dump(res, open(args.out, "w"), indent=1)
        print(f"[margins] wrote {args.out}")


if __name__ == "__main__":
    main()
