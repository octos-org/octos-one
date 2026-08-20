#!/usr/bin/env python3
"""Write a trained checkpoint back out as an sglang-loadable draft model dir.

Inverse of DFlashDraft.load_dflash_checkpoint: our module holds the per-head
q_norm/k_norm as bare Parameters, the checkpoint stores them as
`...q_norm.weight`. Everything else is name-identical to 0e6412a.
"""
import argparse, json, os, shutil, sys
import torch
from safetensors.torch import save_file

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--base", default="/models/draft", help="dir to copy config/tokenizer from")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)
    sd = torch.load(args.ckpt, map_location="cpu", weights_only=True)
    sd = sd.get("model", sd)
    out = {}
    for k, v in sd.items():
        key = k + ".weight" if k.endswith((".q_norm", ".k_norm")) else k
        out[key] = v.to(torch.bfloat16).contiguous()
    save_file(out, os.path.join(args.out, "model.safetensors"),
              metadata={"format": "pt"})
    for f in ("config.json", "README.md"):
        src = os.path.join(args.base, f)
        if os.path.exists(src):
            shutil.copy(src, os.path.join(args.out, f))
    print(f"wrote {len(out)} tensors to {args.out}")
    ref = json.load(open(os.path.join(args.out, "config.json")))
    print("config: block_size", ref.get("block_size"),
          "dflash", ref.get("dflash_config"))


if __name__ == "__main__":
    main()
