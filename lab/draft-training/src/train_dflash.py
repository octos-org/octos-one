#!/usr/bin/env python3
"""Goal 3/5: train the DFlash draft on harvested compositions.

Conditioning is constructed exactly as dflash_worker_v2.py does at serve time
(CONDITIONING.md §6). Cross-entropy on hard labels against the recorded temp-0
target tokens, logits through the FROZEN target lm_head.

Memory plan (single GH200, must stay under ~30GB so production keeps serving):
  draft params            bf16  3.5 GB
  grads                   bf16  3.5 GB
  Adam exp_avg            bf16  3.5 GB
  Adam exp_avg_sq         fp32  6.9 GB   (fp32 because bf16 squares underflow)
  target lm_head          bf16  2.5 GB   (frozen; embed_tokens stays on CPU and
                                          only the ~M rows per step are gathered)
  activations                  ~2-4 GB
"""
from __future__ import annotations
import argparse, json, math, os, random, sys, time, collections

import torch
import torch.nn.functional as F

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dflash_torch import DFlashCfg, DFlashDraft, TargetHeads, MASK_TOKEN_ID
from dataset import FeatureStore, build_batch, sample_anchors


class MixedAdamW:
    """AdamW with bf16 params/grads/exp_avg and fp32 exp_avg_sq.

    torch.optim.AdamW allocates state with zeros_like(param), which would put
    exp_avg_sq in bf16: squared grads there flush to zero and the run silently
    stops learning. Keeping only exp_avg_sq in fp32 costs 6.9GB and fixes it.
    """

    def __init__(self, params, lr=1e-4, betas=(0.9, 0.95), eps=1e-8, wd=0.01):
        self.params = [p for p in params if p.requires_grad]
        self.lr, self.b1, self.b2, self.eps, self.wd = lr, betas[0], betas[1], eps, wd
        self.m = [torch.zeros_like(p) for p in self.params]
        self.v = [torch.zeros(p.shape, dtype=torch.float32, device=p.device)
                  for p in self.params]
        self.t = 0

    def zero_grad(self):
        for p in self.params:
            p.grad = None

    @torch.no_grad()
    def step(self, lr=None):
        self.t += 1
        lr = self.lr if lr is None else lr
        bc1 = 1 - self.b1 ** self.t
        bc2 = 1 - self.b2 ** self.t
        for p, m, v in zip(self.params, self.m, self.v):
            if p.grad is None:
                continue
            g = p.grad
            m.mul_(self.b1).add_(g, alpha=1 - self.b1)
            gf = g.float()
            v.mul_(self.b2).addcmul_(gf, gf, value=1 - self.b2)
            denom = (v / bc2).sqrt_().add_(self.eps)
            upd = (m.float() / bc1) / denom
            if self.wd:
                upd.add_(p.float(), alpha=self.wd)
            p.add_(upd.to(p.dtype), alpha=-lr)

    def state_dict(self):
        return {"t": self.t, "m": self.m, "v": self.v}

    def load_state_dict(self, sd):
        self.t = sd["t"]
        for a, b in zip(self.m, sd["m"]): a.copy_(b)
        for a, b in zip(self.v, sd["v"]): a.copy_(b)


def loss_for_batch(model, heads, batch, chunk=4096):
    hid = model(batch["h"], batch["ctx_pos"], heads.embed_block(batch["ids"]),
                batch["blk_pos"], batch["blk_group"], batch["blk_anchor"])
    m = batch["loss_mask"]
    hid = hid[m]
    lab = batch["labels"][m]
    total, ncorrect = 0.0, 0
    losses = []
    for i in range(0, hid.shape[0], chunk):
        lg = heads.logits(hid[i:i + chunk]).float()
        l = lab[i:i + chunk]
        losses.append(F.cross_entropy(lg, l, reduction="sum"))
        ncorrect += int((lg.argmax(-1) == l).sum())
    n = max(1, hid.shape[0])
    return torch.stack(losses).sum() / n, ncorrect / n, n


def split_holdout(store, frac, seed):
    """Hold out whole sequences, and hold out UNSEEN COMBOS for compose: any
    compose query whose family+city combination never appears in training."""
    rng = random.Random(seed)
    names = [e["name"] for e in store.seqs]
    rng.shuffle(names)
    k = max(1, int(len(names) * frac))
    return set(names[:k]), [n for n in names[k:]]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--feat-dir", default="/mnt/dflash-feats")
    ap.add_argument("--draft", default="/models/draft")
    ap.add_argument("--target", default="/models/target")
    ap.add_argument("--out", default="/home/ubuntu/qwen38-h200/draft-training/ckpt")
    ap.add_argument("--block-size", type=int, default=16)
    ap.add_argument("--block-sizes", default="",
                    help="comma list, e.g. 8,16,32,48: sample the block size per "
                         "group so the draft is not locked to one serving block. "
                         "Empty = always --block-size.")
    ap.add_argument("--window", type=int, default=4096)
    ap.add_argument("--kv-fp8", action="store_true",
                    help="emulate the fp8_e4m3 draft KV cache used at serve time")
    ap.add_argument("--anchors", type=int, default=24, help="draft blocks per forward")
    ap.add_argument("--anchor-span", type=int, default=768)
    ap.add_argument("--accum", type=int, default=2)
    ap.add_argument("--lr", type=float, default=5e-5)
    ap.add_argument("--warmup", type=int, default=50)
    ap.add_argument("--lr-min-frac", type=float, default=0.1,
                    help="cosine-decay the LR to this fraction by the last step")
    ap.add_argument("--epochs", type=float, default=1.0)
    ap.add_argument("--max-steps", type=int, default=0)
    ap.add_argument("--holdout-frac", type=float, default=0.08)
    ap.add_argument("--splits", default=None,
                    help="dir from holdout.py; uses train.json/holdout_all.json "
                         "instead of a random split")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--grad-ckpt", action="store_true")
    ap.add_argument("--debug-tiny", action="store_true",
                    help="random-init miniature model + fake lm_head; exercises the "
                         "whole loop (optimizer, accumulation, clipping, logging, "
                         "checkpointing) in <1GB when the GPU is busy serving")
    ap.add_argument("--dry-run", type=int, default=0,
                    help="N sequences; smoke test only, no checkpoints")
    ap.add_argument("--save-every", type=int, default=400)
    ap.add_argument("--log-every", type=int, default=10)
    args = ap.parse_args()
    device = "cuda"
    torch.manual_seed(args.seed)

    os.makedirs(args.out, exist_ok=True)
    if args.debug_tiny:
        cfg = DFlashCfg(hidden_size=512, num_hidden_layers=2, num_attention_heads=4,
                        num_key_value_heads=2, head_dim=128, intermediate_size=1024,
                        layer_types=("sliding_attention", "full_attention"),
                        num_context_features=25600 // 512,
                        vocab_size=1024, num_org_vocab=1024)
        cfg.block_size = args.block_size
        cfg.ctx_window = args.window
        model = DFlashDraft(cfg).to(device, torch.bfloat16)
        model.grad_ckpt = args.grad_ckpt
        print("[train] DEBUG-TINY: random init, fake lm_head; loop test only, "
              "the numbers mean nothing")
    else:
        cfg = DFlashCfg.from_model_dir(args.draft)
        cfg.block_size = args.block_size
        cfg.ctx_window = args.window   # == serve-time --speculative-draft-window-size
        cfg.kv_fp8 = args.kv_fp8       # the draft KV pool is fp8_e4m3 at serve time
        model = DFlashDraft(cfg).to(device, torch.bfloat16)
        n, unused = model.load_dflash_checkpoint(args.draft)
        model.grad_ckpt = args.grad_ckpt
        print(f"[train] init from {args.draft}: {n} tensors"
              + (f", unused: {unused}" if unused else ""))
    nparam = sum(p.numel() for p in model.parameters())
    print(f"[train] trainable params: {nparam/1e9:.3f}B")

    if args.debug_tiny:
        heads = TargetHeads(
            torch.randn(cfg.vocab_size, cfg.hidden_size, dtype=torch.bfloat16) * 0.02,
            torch.randn(cfg.vocab_size, cfg.hidden_size, dtype=torch.bfloat16,
                        device=device) * 0.02,
            cfg.vocab_size, out_device=device)
    else:
        # embed_tokens stays on the host (2.5GB, a few hundred rows touched/step)
        heads = TargetHeads.load(args.target, device=device, embed_on_cpu=True)

    store = FeatureStore(args.feat_dir, cache_size=3)
    if args.splits:
        have = {e["name"] for e in store.seqs}
        hold = set(json.load(open(os.path.join(args.splits, "holdout_all.json")))) & have
        train_names = [n for n in json.load(open(os.path.join(args.splits, "train.json")))
                       if n in have]
        print(f"[train] using splits from {args.splits}")
    else:
        hold, train_names = split_holdout(store, args.holdout_frac, args.seed)
    if args.dry_run:
        train_names = train_names[: args.dry_run]
    json.dump(sorted(hold), open(os.path.join(args.out, "holdout.json"), "w"))
    print(f"[train] {len(train_names)} train seqs, {len(hold)} held out")

    block_sizes = ([int(x) for x in args.block_sizes.split(",")]
                   if args.block_sizes else [args.block_size])
    print(f"[train] block sizes sampled per group: {block_sizes}")
    opt = MixedAdamW(model.parameters(), lr=args.lr)
    rng = random.Random(args.seed)
    order = list(train_names)
    steps_per_epoch = max(1, len(order) // args.accum)
    total_steps = args.max_steps or int(steps_per_epoch * args.epochs)
    print(f"[train] {total_steps} optimizer steps "
          f"({args.accum} seqs/step, {args.anchors} blocks/seq)")

    torch.cuda.reset_peak_memory_stats()
    t0 = time.time()
    step = 0
    hist = collections.deque(maxlen=50)
    log_path = os.path.join(args.out, "trainlog.jsonl")
    logf = open(log_path, "a")
    ep = 0
    while step < total_steps:
        rng.shuffle(order)
        ep += 1
        for i in range(0, len(order), args.accum):
            if step >= total_steps:
                break
            group = order[i:i + args.accum]
            opt.zero_grad()
            tot_loss, tot_acc, nb = 0.0, 0.0, 0
            for name in group:
                B = rng.choice(block_sizes)
                anchors = sample_anchors(store, name, args.anchors, B,
                                         args.window, rng, span=args.anchor_span)
                if not anchors:
                    continue
                batch = build_batch(store, name, anchors, args.window,
                                    B, device=device)
                if args.debug_tiny:
                    batch["ids"] = batch["ids"] % cfg.vocab_size
                    batch["labels"] = batch["labels"] % cfg.vocab_size
                loss, acc, ntok = loss_for_batch(model, heads, batch)
                (loss / max(1, len(group))).backward()
                tot_loss += loss.detach().item(); tot_acc += acc; nb += 1
            if nb == 0:
                continue
            gn = torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            if step + 1 <= args.warmup:
                lr = args.lr * (step + 1) / max(1, args.warmup)
            else:
                prog = (step + 1 - args.warmup) / max(1, total_steps - args.warmup)
                lr = args.lr * (args.lr_min_frac + (1 - args.lr_min_frac)
                                * 0.5 * (1 + math.cos(math.pi * min(1.0, prog))))
            opt.step(lr=lr)
            step += 1
            hist.append(tot_loss / nb)
            if step % args.log_every == 0 or step == 1:
                peak = torch.cuda.max_memory_allocated() / 2**30
                rec = {"step": step, "epoch": ep, "loss": tot_loss / nb,
                       "loss_avg50": sum(hist) / len(hist),
                       "tok_acc": tot_acc / nb, "grad_norm": float(gn),
                       "lr": lr, "peak_gb": round(peak, 2),
                       "sec": round(time.time() - t0, 1)}
                print(json.dumps(rec), flush=True)
                logf.write(json.dumps(rec) + "\n"); logf.flush()
            if not args.dry_run and args.save_every and step % args.save_every == 0:
                p = os.path.join(args.out, f"draft_step{step}.pt")
                torch.save({"model": model.state_dict(), "step": step,
                            "args": vars(args)}, p)
                print(f"[train] saved {p}", flush=True)

    peak = torch.cuda.max_memory_allocated() / 2**30
    print(f"[train] done: {step} steps, {time.time()-t0:.0f}s, peak HBM {peak:.2f} GB")
    if not args.dry_run:
        p = os.path.join(args.out, "draft_final.pt")
        torch.save({"model": model.state_dict(), "step": step, "args": vars(args)}, p)
        print(f"[train] saved {p}")
    else:
        first = list(hist)[:1]
        print(f"[train] DRY RUN: first loss {first}, last {list(hist)[-1] if hist else None}")


if __name__ == "__main__":
    main()
