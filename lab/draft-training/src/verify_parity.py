#!/usr/bin/env python3
"""Prove src/dflash_torch.py reproduces the real serving stack token-for-token.

CLAUDE.md names train/serve conditioning mismatch as the classic failure mode, so
this is the check that actually retires that risk: it compares the draft tokens
proposed by sglang's own DFLASH worker against the ones our reimplementation
proposes from the same context.

Data comes for free from the extraction run when it is launched with
XSTEPS=1: every request there is one prefill (dumped as pf_*.pt, and assembled
into the feature store) followed by exactly one real DFLASH decode step
(dumped as st_*.pt with the true block ids, positions and proposals).

For the comparison to be meaningful the extraction server must run with
--speculative-draft-window-size == the window used here (launch_extract.sh
XWINDOW), otherwise sglang's full-attention layer 4 sees the whole 53k context
and ours sees only the window.
"""
from __future__ import annotations
import argparse, glob, json, os, sys, collections

import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dflash_torch import DFlashCfg, DFlashDraft, TargetHeads, MASK_TOKEN_ID
from dataset import FeatureStore


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dump-dir", default="/mnt/dflash-dump")
    ap.add_argument("--feat-dir", default="/mnt/dflash-feats")
    ap.add_argument("--draft", default="/models/draft")
    ap.add_argument("--target", default="/models/target")
    ap.add_argument("--window", type=int, default=4096)
    ap.add_argument("--kv-fp8", action="store_true",
                    help="emulate the fp8_e4m3 KV cache the draft actually uses")
    ap.add_argument("--max", type=int, default=40)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    device = "cuda"

    man = [json.loads(l) for l in open(os.path.join(args.dump_dir, "manifest.jsonl")) if l.strip()]
    sgl2name = {e.get("sgl_rid"): e["rid"] for e in man if e["kind"] == "seq"}

    steps = sorted(glob.glob(os.path.join(args.dump_dir, "st_*.pt")))
    if not steps:
        raise SystemExit(f"no st_*.pt in {args.dump_dir}; relaunch extraction with XSTEPS=1")
    print(f"[parity] {len(steps)} dumped decode steps")

    cfg = DFlashCfg.from_model_dir(args.draft)
    cfg.ctx_window = args.window
    cfg.kv_fp8 = args.kv_fp8
    model = DFlashDraft(cfg).to(device, torch.bfloat16).eval()
    model.load_dflash_checkpoint(args.draft)
    heads = TargetHeads.load(args.target, device=device)
    store = FeatureStore(args.feat_dir, cache_size=2)

    B = cfg.block_size
    margins, allmarg = [], []
    n_cmp = n_exact = 0
    tok_tot = tok_match = 0
    first_div = collections.Counter()
    examples = []
    for f in steps:
        if n_cmp >= args.max:
            break
        d = torch.load(f, map_location="cpu", weights_only=True)
        for row, rid in enumerate(d["rids"]):
            name = sgl2name.get(rid)
            if name is None or name not in store.by_name:
                continue
            pos = d["positions"][row].tolist()
            anchor = int(pos[0])
            blk_ids = d["block_ids"][row]
            ref = d["draft_tokens"][row][1:]                # sglang's proposals
            s0, s1 = store.span(name)
            lo = max(store.ctx_floor(name), anchor - args.window)
            if anchor > s1 or lo >= anchor:
                continue
            h, cpos = store.context(name, lo, anchor)
            h = h.to(device, torch.bfloat16)
            cpos = cpos.to(device)
            ids = blk_ids.to(device).long()
            out = model(h, cpos, heads.embed_block(ids),
                        torch.tensor(pos, device=device),
                        torch.zeros(len(pos), dtype=torch.long, device=device),
                        torch.full((len(pos),), anchor, device=device))
            lg = heads.logits(out[1:]).float()
            mine = lg.argmax(-1).cpu()
            same = (mine == ref)
            # Decisive diagnostic: at each disagreement, how much logit does our
            # pick beat sglang's by? A structural conditioning error produces a
            # confident wrong token; bf16/kernel noise only flips near-ties.
            if not same.all():
                bad = (~same).nonzero().flatten()
                top = lg.gather(1, mine.to(lg.device)[:, None]).squeeze(1)
                theirs = lg.gather(1, ref.to(lg.device)[:, None]).squeeze(1)
                margins.extend((top - theirs)[bad.to(lg.device)].tolist())
            allmarg.extend((lg.max(-1).values
                            - lg.topk(2, dim=-1).values[:, 1]).tolist())
            tok_tot += same.numel(); tok_match += int(same.sum())
            n_cmp += 1
            if bool(same.all()):
                n_exact += 1
            else:
                fd = int((~same).nonzero()[0].item())
                first_div[fd] += 1
                if len(examples) < 5:
                    examples.append({"name": name, "anchor": anchor,
                                     "first_divergence": fd,
                                     "sglang": ref.tolist(), "ours": mine.tolist()})
            if n_cmp >= args.max:
                break

    import statistics as st
    res = {"blocks_compared": n_cmp,
           "divergence_logit_margin": {
               "n": len(margins),
               "mean": (sum(margins) / len(margins)) if margins else None,
               "median": st.median(margins) if margins else None,
               "p90": (sorted(margins)[int(0.9 * (len(margins) - 1))] if margins else None),
               "max": max(margins) if margins else None,
           },
           "all_slots_top1_top2_gap_median": st.median(allmarg) if allmarg else None,
           "blocks_identical": n_exact,
           "token_agreement": tok_match / max(1, tok_tot),
           "tokens": tok_tot,
           "first_divergence_hist": dict(sorted(first_div.items())),
           "examples": examples,
           "window": args.window, "block_size": B}
    print(json.dumps({k: v for k, v in res.items() if k != "examples"}, indent=1))
    if examples:
        print("first mismatching blocks:")
        for e in examples:
            print(f"  {e['name']} @ {e['anchor']} diverges at draft slot {e['first_divergence']}")
            print(f"    sglang {e['sglang']}")
            print(f"    ours   {e['ours']}")
    if args.out:
        json.dump(res, open(args.out, "w"), indent=1)
    ok = n_cmp > 0 and n_exact == n_cmp
    print(("PARITY OK" if ok else "PARITY IMPERFECT")
          + f": {n_exact}/{n_cmp} blocks identical, "
            f"{100*tok_match/max(1,tok_tot):.2f}% token agreement")


if __name__ == "__main__":
    main()
