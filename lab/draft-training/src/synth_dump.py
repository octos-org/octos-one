#!/usr/bin/env python3
"""Fabricate a small dump dir shaped exactly like a real extraction run, so
assemble.py / dataset.py / train_dflash.py / eval_accept.py can be exercised
end-to-end without occupying the GPU with the 27B target.

Mimics the real geometry: a long shared prompt prefix, per-sequence tails that
start at the divergence point, and prefill split across two chunks.
"""
import argparse, json, os, random
import torch

DIM = 25600


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dump-dir", default="/tmp/synth-dump")
    ap.add_argument("--n-seq", type=int, default=6)
    ap.add_argument("--shared", type=int, default=5000)
    ap.add_argument("--prompt-extra", type=int, default=12)
    ap.add_argument("--gen", type=int, default=400)
    ap.add_argument("--window", type=int, default=1024)
    ap.add_argument("--vocab", type=int, default=248044)
    args = ap.parse_args()
    os.makedirs(args.dump_dir, exist_ok=True)
    rng = random.Random(0)
    g = torch.Generator().manual_seed(0)
    man = open(os.path.join(args.dump_dir, "manifest.jsonl"), "w")
    sq = 0

    def dump(rid, pos_lo, pos_hi):
        nonlocal sq
        names = []
        # split into two chunks like chunked prefill does
        mid = (pos_lo + pos_hi) // 2
        for a, b in ((pos_lo, mid), (mid, pos_hi)):
            if b <= a:
                continue
            sq += 1
            fn = f"pf_{sq:07d}.pt"
            names.append(fn)
            torch.save({"rid": rid,
                        "pos": torch.arange(a, b, dtype=torch.int64),
                        "h": (torch.randn(b - a, DIM, generator=g) * 0.1).to(torch.bfloat16)},
                       os.path.join(args.dump_dir, fn))
        return names

    for mode in ("pick", "compose"):
        shared = args.shared + (60 if mode == "compose" else 0)
        keep = max(0, shared - args.window)
        rid = f"synthprefix-{mode}"
        pfiles = dump(rid, keep, shared)
        man.write(json.dumps({"kind": "prefix", "mode": mode, "rid": rid,
                              "files": pfiles,
                              "shared_len": shared, "keep_from": keep,
                              "ids": [rng.randrange(args.vocab) for _ in range(shared - keep)]}) + "\n")
        for i in range(args.n_seq):
            plen = shared + args.prompt_extra
            total = plen + args.gen
            srid = f"sgl-{mode}-{i}"
            sfiles = dump(srid, shared, total)
            man.write(json.dumps({
                "kind": "seq", "rid": f"{mode}-{i:03d}", "sgl_rid": srid,
                "files": sfiles,
                "mode": mode, "family": "synth", "query": f"q{i}",
                "shared_len": shared, "prompt_len": plen, "total_len": total,
                "ids": [rng.randrange(args.vocab) for _ in range(total - shared)],
            }) + "\n")
    man.close()
    print(f"synth dump written to {args.dump_dir}: {sq} shards")


if __name__ == "__main__":
    main()
