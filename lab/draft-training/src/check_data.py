#!/usr/bin/env python3
"""Assert the training/eval data path builds exactly the conditioning described
in CONDITIONING.md §6. CPU-only -- no model, no GPU.

Every one of these is a silent-failure mode if wrong: a bad label offset or a
context that includes the anchor still trains, still shows a falling loss, and
still produces a draft that is useless at serve time.
"""
import argparse, os, sys
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dataset import FeatureStore, build_batch, sample_anchors
from dflash_torch import MASK_TOKEN_ID
import random


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--feat-dir", default="/synth/feats")
    ap.add_argument("--window", type=int, default=1024)
    ap.add_argument("--block-size", type=int, default=16)
    ap.add_argument("--anchors", type=int, default=8)
    ap.add_argument("--span", type=int, default=256)
    ap.add_argument("--n-seq", type=int, default=6)
    args = ap.parse_args()

    store = FeatureStore(args.feat_dir, cache_size=4)
    print(f"[data] {len(store.seqs)} sequences, prefixes: {sorted(store.prefix)}")
    rng = random.Random(0)
    checks = 0
    for e in store.seqs[: args.n_seq]:
        name = e["name"]
        g0, g1 = store.gen_span(name)
        s0, s1 = store.span(name)
        assert s0 == e["shared_len"] if "shared_len" in e else True
        anchors = sample_anchors(store, name, args.anchors, args.block_size,
                                 args.window, rng, span=args.span)
        assert anchors, name
        b = build_batch(store, name, anchors, args.window, args.block_size, device="cpu")
        B = args.block_size

        # -- context is contiguous, ends before the last anchor, starts >= floor
        cp = b["ctx_pos"]
        assert torch.equal(cp, torch.arange(int(cp[0]), int(cp[-1]) + 1)), "ctx not contiguous"
        assert int(cp[-1]) < anchors[-1], "ctx reaches the last anchor"
        assert int(cp[0]) >= store.ctx_floor(name), "ctx starts before available features"
        assert b["h"].shape[0] == cp.numel(), "ctx features/positions length mismatch"

        # -- context features really are the ones for those positions
        h2, p2 = store.context(name, int(cp[0]), int(cp[-1]) + 1)
        assert torch.equal(p2, cp) and torch.equal(h2, b["h"]), "context stitch unstable"

        for gi, a in enumerate(anchors):
            sl = slice(gi * B, (gi + 1) * B)
            ids, pos = b["ids"][sl], b["blk_pos"][sl]
            lab, lm = b["labels"][sl], b["loss_mask"][sl]
            true = store.tokens(name, a, a + B)
            assert int(ids[0]) == int(true[0]), "block slot 0 must be the bonus token t[a]"
            assert torch.all(ids[1:] == MASK_TOKEN_ID), "block slots 1.. must all be MASK"
            assert torch.equal(pos, torch.arange(a, a + B)), "block positions must be absolute"
            assert torch.equal(lab, true), "labels must be t[a .. a+B)"
            assert not bool(lm[0]) and bool(lm[1:].all()), "slot 0 must be excluded from loss"
            assert torch.all(b["blk_anchor"][sl] == a) and torch.all(b["blk_group"][sl] == gi)
            checks += 1

        # -- the window really is W deep for an anchor far enough in
        a = anchors[-1]
        lo = max(store.ctx_floor(name), a - args.window)
        h3, p3 = store.context(name, lo, a)
        assert int(p3[-1]) == a - 1, "context must end at anchor-1"
        assert p3.numel() == a - lo

        # -- prefix/sequence stitch boundary is seamless
        if s0 - 4 > store.ctx_floor(name):
            hb, pb = store.context(name, s0 - 4, s0 + 4)
            assert torch.equal(pb, torch.arange(s0 - 4, s0 + 4))
            pre = store.prefix[e["mode"]]
            assert torch.equal(hb[:4], pre["h"][s0 - 4 - pre["pos0"]: s0 - pre["pos0"]])
            assert torch.equal(hb[4:], store.seq(name)["h"][:4])

        # -- token lookup across the boundary matches both sources
        tb = store.tokens(name, s0 - 3, s0 + 3)
        pre = store.prefix[e["mode"]]
        assert torch.equal(tb[:3], pre["ids"][s0 - 3 - pre["pos0"]: s0 - pre["pos0"]].long())
        assert torch.equal(tb[3:], store.seq(name)["ids"][:3].long())

    print(f"[data] OK: {checks} draft blocks checked across {min(args.n_seq, len(store.seqs))} sequences")
    print("[data] verified: bonus token at slot 0, MASK elsewhere, absolute positions, "
          "labels t[a..a+B), slot 0 excluded from loss, context strictly < anchor, "
          "prefix/sequence stitch seamless")


if __name__ == "__main__":
    main()
