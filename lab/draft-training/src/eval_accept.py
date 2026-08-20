#!/usr/bin/env python3
"""Goal 4: accept-length simulator.

Replays held-out recorded generations through the DFlash draft exactly as the
serving loop would, and counts leading exact matches.

Semantics (matches dkern:_dflash_accept_bonus_contig_kernel):
  candidates = [bonus, d_1 .. d_{B-1}]  where bonus = t[a] is already verified
  accept_len = longest prefix with d_j == target_top1_{j-1}
  commit_len = accept_len + 1           (drafts + the bonus/correction token)
So per verify step at most B-1 = 15 tokens can come from the draft.

Target top-1 is taken to be the recorded temp-0 token t[a+j]. That is exact when
the recorded generation is a greedy fixpoint of the tokenization, which
--check-fixpoint measures.

Reported per slice:
  accept/verify      mean accept_len          (0 .. B-1)   <- the speed number
  tokens/verify      mean commit_len = accept+1
  K-window           of the first K committed tokens starting at an anchor, how
                     many came from accepted drafts. This is the "40/48" gate:
                     covering K tokens costs >= ceil(K/B) verifies, so the
                     ceiling is K - ceil(K/B).
"""
from __future__ import annotations
import argparse, json, os, random, re, sys, time, collections

import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dflash_torch import DFlashCfg, DFlashDraft, TargetHeads, MASK_TOKEN_ID
from dataset import FeatureStore

SECTION_RE = re.compile(
    r"(?m)^(?:\s{0,6})(source\s|state\s|Panel\b|Card\b|Section\b|Col\(|Row\(|Header\b|Group\b|#\s*model:|```)")


def seam_positions(store: FeatureStore, name: str, tok) -> set:
    """Absolute token positions that start a new top-level card section."""
    g0, g1 = store.gen_span(name)
    s0, s1 = store.span(name)
    g0, g1 = max(g0, s0), min(g1, s1)
    ids = store.tokens(name, g0, g1).tolist()
    pieces = tok.convert_ids_to_tokens(ids)
    text, offs = [], []
    cur = 0
    for i, tid in enumerate(ids):
        s = tok.decode([tid])
        offs.append(cur)
        text.append(s)
        cur += len(s)
    text = "".join(text)
    seams = set()
    for m in SECTION_RE.finditer(text):
        c = m.start()
        lo, hi = 0, len(offs) - 1
        while lo < hi:
            mid = (lo + hi) // 2
            if offs[mid] < c:
                lo = mid + 1
            else:
                hi = mid
        seams.add(g0 + lo)
    return seams


def k_window_accept(acc_hist, K):
    """Of the first K tokens committed from an anchor, how many were accepted
    drafts. Each verify commits accept+1 tokens (the +1 is the target's own
    token), so covering K tokens costs at least ceil(K/B) verifies and the
    ceiling on this metric is K - ceil(K/B). None if the chain ran out of
    sequence before covering K.
    """
    got = tot = 0
    for acc in acc_hist:
        commit = acc + 1
        if tot + commit >= K:
            return got + min(acc, K - tot - 1)
        got += acc
        tot += commit
    return None


@torch.no_grad()
def run_sequence(model, heads, store, name, starts, B, W, K_list, device):
    """Run one chained draft loop per start anchor, all in shared forwards."""
    Kmax = max(K_list)
    g0, g1 = store.gen_span(name)
    s0, s1 = store.span(name)
    g1 = min(g1, s1)
    chains = []
    for a in starts:
        if a + Kmax + B >= g1:
            continue
        chains.append({"start": a, "a": a, "committed": 0,
                       "steps": 0, "acc_hist": []})
    if not chains:
        return []

    floor = store.ctx_floor(name)
    while True:
        active = [c for c in chains if c["committed"] < Kmax and c["a"] + B < g1]
        if not active:
            break
        anchors = [c["a"] for c in active]
        lo = max(floor, min(anchors) - W)
        hi = max(anchors)
        h, cpos = store.context(name, lo, hi)
        h = h.to(device, torch.bfloat16, non_blocking=True)
        cpos = cpos.to(device, non_blocking=True)

        ids, bpos, grp, anc = [], [], [], []
        labels = []
        for gi, c in enumerate(active):
            a = c["a"]
            toks = store.tokens(name, a, a + B)
            blk = torch.full((B,), MASK_TOKEN_ID, dtype=torch.long)
            blk[0] = toks[0]
            ids.append(blk); labels.append(toks)
            bpos.append(torch.arange(a, a + B))
            grp.append(torch.full((B,), gi)); anc.append(torch.full((B,), a))
        ids_t = torch.cat(ids).to(device)
        emb = heads.embed_block(ids_t)
        out = model(h, cpos, emb, torch.cat(bpos).to(device),
                    torch.cat(grp).to(device), torch.cat(anc).to(device))
        out = out.view(len(active), B, -1)[:, 1:, :]           # drop block pos 0
        drafts = heads.logits(out.reshape(-1, out.shape[-1])).argmax(-1).view(len(active), B - 1)
        drafts = drafts.cpu()

        for gi, c in enumerate(active):
            tgt = labels[gi][1:]                                # t[a+1 .. a+B-1]
            d = drafts[gi]
            eq = (d == tgt)
            acc = int((~eq).nonzero()[0].item()) if (~eq).any() else int(B - 1)
            commit = acc + 1
            c["acc_hist"].append(acc)
            c["steps"] += 1
            c["committed"] += commit
            c["a"] += commit
    out = []
    for c in chains:
        rec = {"name": name, "start": c["start"],
               "acc_hist": c["acc_hist"], "steps": c["steps"]}
        for K in K_list:
            rec[f"acc@{K}"] = k_window_accept(c["acc_hist"], K)
        out.append(rec)
    return out


def summarize(rows, K_list):
    if not rows:
        return {}
    hist = [a for r in rows for a in r["acc_hist"]]
    s = {"n_windows": len(rows), "n_verifies": len(hist),
         "accept_per_verify": sum(hist) / max(1, len(hist)),
         "tokens_per_verify": (sum(hist) + len(hist)) / max(1, len(hist))}
    for K in K_list:
        v = [r[f"acc@{K}"] for r in rows if r.get(f"acc@{K}") is not None]
        s[f"acc@{K}"] = sum(v) / len(v) if v else None
        s[f"n@{K}"] = len(v)
    return s


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--feat-dir", default="/mnt/dflash-feats")
    ap.add_argument("--draft", default="/models/draft", help="dir with model.safetensors + config.json")
    ap.add_argument("--ckpt", default=None, help="optional trained .pt state_dict to load instead")
    ap.add_argument("--target", default="/models/target")
    ap.add_argument("--block-size", type=int, default=16)
    ap.add_argument("--window", type=int, default=4096)
    ap.add_argument("--kv-fp8", action="store_true",
                    help="emulate the fp8_e4m3 draft KV cache used at serve time")
    ap.add_argument("--ks", default="8,16,32,48")
    ap.add_argument("--per-seq", type=int, default=6, help="start anchors per sequence")
    ap.add_argument("--max-seqs", type=int, default=0)
    ap.add_argument("--holdout", default=None, help="json list of held-out sequence names")
    ap.add_argument("--slices", default=None,
                    help="json map name -> slice label (from holdout.py); adds "
                         "cards / unseen_combos / general rows to the report")
    ap.add_argument("--seams", action="store_true", help="also report seam-window acceptance")
    ap.add_argument("--seam-radius", type=int, default=10)
    ap.add_argument("--out", default=None)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()
    K_list = [int(x) for x in args.ks.split(",")]
    device = "cuda"

    cfg = DFlashCfg.from_model_dir(args.draft)
    cfg.ctx_window = args.window   # == serve-time --speculative-draft-window-size
    cfg.kv_fp8 = args.kv_fp8       # the draft KV pool is fp8_e4m3 at serve time
    model = DFlashDraft(cfg).to(device, torch.bfloat16).eval()
    n, unused = model.load_dflash_checkpoint(args.draft)
    print(f"[eval] loaded {n} draft params from {args.draft}"
          + (f" (unused ckpt tensors: {unused})" if unused else ""))
    if args.ckpt:
        sd = torch.load(args.ckpt, map_location="cpu", weights_only=True)
        sd = sd.get("model", sd)
        model.load_state_dict({k: v.to(torch.bfloat16) for k, v in sd.items()}, strict=True)
        print(f"[eval] overrode weights from {args.ckpt}")
    heads = TargetHeads.load(args.target, device=device)
    store = FeatureStore(args.feat_dir, cache_size=2)

    names = [e["name"] for e in store.seqs]
    if args.holdout:
        keep = set(json.load(open(args.holdout)))
        names = [n for n in names if n in keep]
    rng = random.Random(args.seed)
    rng.shuffle(names)
    if args.max_seqs:
        names = names[: args.max_seqs]
    print(f"[eval] {len(names)} sequences, block={args.block_size}, W={args.window}")

    slice_map = json.load(open(args.slices)) if args.slices else {}

    tok = None
    if args.seams:
        from transformers import AutoTokenizer
        tok = AutoTokenizer.from_pretrained(args.target, trust_remote_code=True)

    by_slice = collections.defaultdict(list)
    seam_rows = collections.defaultdict(list)
    t0 = time.time()
    for i, name in enumerate(names):
        e = store.by_name[name]
        g0, g1 = store.gen_span(name)
        s0, s1 = store.span(name)
        g1 = min(g1, s1)
        room = g1 - g0 - max(K_list) - args.block_size
        if room <= 0:
            continue
        starts = sorted(rng.sample(range(g0, g0 + room), min(args.per_seq, room)))
        rows = run_sequence(model, heads, store, name, starts,
                            args.block_size, args.window, K_list, device)
        by_slice[e["mode"]].extend(rows)
        by_slice["ALL"].extend(rows)
        by_slice[f"{e['mode']}/{e['family']}"].extend(rows)
        lab = slice_map.get(name)
        if lab:
            by_slice[f"GATE:{lab}"].extend(rows)
        if args.seams and tok is not None:
            seams = seam_positions(store, name, tok)
            sstarts = sorted({p for p in seams
                              if g0 <= p < g1 - max(K_list) - args.block_size})
            if sstarts:
                sstarts = rng.sample(sstarts, min(args.per_seq, len(sstarts)))
                srows = run_sequence(model, heads, store, name, sorted(sstarts),
                                     args.block_size, args.window, K_list, device)
                seam_rows[e["mode"]].extend(srows)
                seam_rows["ALL"].extend(srows)
                if lab:
                    seam_rows[f"GATE:{lab}"].extend(srows)
        if (i + 1) % 20 == 0:
            print(f"  {i+1}/{len(names)}  {time.time()-t0:.0f}s", flush=True)

    result = {"config": vars(args),
              "slices": {k: summarize(v, K_list) for k, v in sorted(by_slice.items())},
              "seams": {k: summarize(v, K_list) for k, v in sorted(seam_rows.items())}}
    print(json.dumps(result["slices"], indent=1))
    if result["seams"]:
        print("SEAM WINDOWS:")
        print(json.dumps(result["seams"], indent=1))
    if args.out:
        json.dump(result, open(args.out, "w"), indent=1)
        print(f"[eval] wrote {args.out}")


if __name__ == "__main__":
    main()
