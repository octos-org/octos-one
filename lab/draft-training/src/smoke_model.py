#!/usr/bin/env python3
"""Smoke test for src/dflash_torch.py with synthetic features.

Checks, without needing the target model or any extracted data:
  1. the 0e6412a checkpoint loads into the reimplementation with no leftovers
  2. a grouped multi-anchor forward runs and produces finite logits
  3. the grouped forward is NUMERICALLY IDENTICAL to running each anchor alone
     (this is the property that makes multi-anchor batching safe)
  4. an anchor's block cannot see context at or beyond its own anchor
     (perturbing ctx rows >= anchor must not change that block's output)
  5. loss.backward() reaches fc / hidden_norm / every layer
  6. peak HBM for a realistic train step
"""
import argparse, os, sys, time
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dflash_torch import DFlashCfg, DFlashDraft, MASK_TOKEN_ID


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--draft", default="/models/draft")
    ap.add_argument("--window", type=int, default=4096)
    ap.add_argument("--anchors", type=int, default=24)
    ap.add_argument("--span", type=int, default=768)
    ap.add_argument("--block-size", type=int, default=16)
    ap.add_argument("--train-step", action="store_true")
    ap.add_argument("--parity-only", action="store_true",
                    help="run only the fp32 grouped-vs-solo check (needs its own "
                         "process: fp32 weights do not fit next to the bf16 ones)")
    args = ap.parse_args()
    dev = "cuda"

    cfg = DFlashCfg.from_model_dir(args.draft)
    cfg.ctx_window = args.window
    print(f"cfg: layers={cfg.num_hidden_layers} hidden={cfg.hidden_size} "
          f"heads={cfg.num_attention_heads}/{cfg.num_key_value_heads} "
          f"hd={cfg.head_dim} sw={cfg.sliding_window} types={cfg.layer_types} "
          f"block={cfg.block_size} feats={cfg.num_context_features} "
          f"mask_id={cfg.mask_token_id} rope={cfg.rope_theta}")
    assert cfg.mask_token_id == MASK_TOKEN_ID, cfg.mask_token_id

    if args.parity_only:
        return parity(cfg, args, dev)

    m = DFlashDraft(cfg).to(dev, torch.bfloat16)
    n, unused = m.load_dflash_checkpoint(args.draft)
    print(f"[1] loaded {n} params; unused checkpoint tensors: {unused}")
    assert not unused, unused
    print(f"    param count {sum(p.numel() for p in m.parameters())/1e9:.3f}B, "
          f"weights {torch.cuda.memory_allocated()/2**30:.2f} GB")

    B, W = args.block_size, args.window
    base = 50000
    anchors = [base + i * (args.span // max(1, args.anchors)) for i in range(args.anchors)]
    lo, hi = anchors[0] - W, anchors[-1]
    T = hi - lo
    g = torch.Generator(device=dev).manual_seed(0)
    H = (torch.randn(T, cfg.num_context_features * cfg.hidden_size, generator=g,
                     device=dev, dtype=torch.float32) * 0.5).to(torch.bfloat16)
    cpos = torch.arange(lo, hi, device=dev)

    ids, bpos, grp, anc = [], [], [], []
    for gi, a in enumerate(anchors):
        blk = torch.full((B,), MASK_TOKEN_ID, dtype=torch.long, device=dev)
        blk[0] = 1234 + gi
        ids.append(blk)
        bpos.append(torch.arange(a, a + B, device=dev))
        grp.append(torch.full((B,), gi, device=dev))
        anc.append(torch.full((B,), a, device=dev))
    ids = torch.cat(ids); bpos = torch.cat(bpos)
    grp = torch.cat(grp); anc = torch.cat(anc)
    emb = (torch.randn(ids.shape[0], cfg.hidden_size, generator=g, device=dev,
                       dtype=torch.float32) * 0.02).to(torch.bfloat16)

    torch.cuda.reset_peak_memory_stats()
    t0 = time.time()
    with torch.no_grad():
        out = m(H, cpos, emb, bpos, grp, anc)
    torch.cuda.synchronize()
    print(f"[2] grouped forward {tuple(out.shape)} in {time.time()-t0:.2f}s, "
          f"finite={torch.isfinite(out).all().item()}, "
          f"peak {torch.cuda.max_memory_allocated()/2**30:.2f} GB")

    # [4] leakage probe: corrupt ctx rows >= anchor of block 0
    gi = 0
    a = anchors[gi]
    H2 = H.clone()
    H2[cpos >= a] = 0
    with torch.no_grad():
        out2 = m(H2, cpos, emb, bpos, grp, anc)
    d0 = (out2[0:B].float() - out[0:B].float()).abs().max().item()
    dl = (out2[-B:].float() - out[-B:].float()).abs().max().item()
    print(f"[4] zeroing ctx >= anchor0: block0 delta {d0:.3e} (must be 0), "
          f"last block delta {dl:.3e} (must be > 0) -> "
          f"{'OK' if d0 == 0.0 and dl > 0 else 'LEAK'}")

    # [5]/[6] backward
    if args.train_step:
        torch.cuda.reset_peak_memory_stats()
        m.train()
        out = m(H, cpos, emb, bpos, grp, anc)
        loss = out.float().pow(2).mean()
        loss.backward()
        touched = [k for k, p in m.named_parameters() if p.grad is not None]
        missing = [k for k, p in m.named_parameters() if p.grad is None]
        print(f"[5] grads on {len(touched)}/{len(touched)+len(missing)} params"
              + (f"; MISSING {missing[:5]}" if missing else ""))
        print(f"[6] peak HBM through backward: "
              f"{torch.cuda.max_memory_allocated()/2**30:.2f} GB")


def parity(cfg, args, dev):
    """[3] grouped == per-anchor, in fp32. In bf16 the two paths reduce over a
    different number of keys, so they differ by ~4e-3 relative rounding, which
    would mask a real logic error."""
    B, W = args.block_size, args.window
    anchors = [50000 + i * (args.span // max(1, args.anchors)) for i in range(args.anchors)]
    lo, hi = anchors[0] - W, anchors[-1]
    m32 = DFlashDraft(cfg).to(dev, torch.float32)
    m32.load_dflash_checkpoint(args.draft)
    g = torch.Generator(device=dev).manual_seed(0)
    H = torch.randn(hi - lo, cfg.num_context_features * cfg.hidden_size,
                    generator=g, device=dev) * 0.5
    cpos = torch.arange(lo, hi, device=dev)
    ids, bpos, grp, anc = [], [], [], []
    for gi, a in enumerate(anchors):
        blk = torch.full((B,), MASK_TOKEN_ID, dtype=torch.long, device=dev)
        blk[0] = 1234 + gi
        ids.append(blk); bpos.append(torch.arange(a, a + B, device=dev))
        grp.append(torch.full((B,), gi, device=dev)); anc.append(torch.full((B,), a, device=dev))
    ids = torch.cat(ids); bpos = torch.cat(bpos); grp = torch.cat(grp); anc = torch.cat(anc)
    emb = torch.randn(ids.shape[0], cfg.hidden_size, generator=g, device=dev) * 0.02
    worst = 0.0
    with torch.no_grad():
        ref = m32(H, cpos, emb, bpos, grp, anc)
        for gi in range(len(anchors)):
            a = anchors[gi]
            sl = slice(gi * B, (gi + 1) * B)
            keep = (cpos >= max(lo, a - W)) & (cpos < a)
            solo = m32(H[keep], cpos[keep], emb[sl], bpos[sl],
                       torch.zeros(B, dtype=torch.long, device=dev),
                       torch.full((B,), a, device=dev))
            worst = max(worst, (solo - ref[sl]).abs().max().item())
    scale = ref.abs().max().item()
    print(f"[3] grouped vs solo (fp32, ctx_window={cfg.ctx_window}) "
          f"max abs diff {worst:.3e} rel {worst/scale:.2e} "
          f"({'OK' if worst / scale < 1e-4 else 'MISMATCH'})")


if __name__ == "__main__":
    main()
