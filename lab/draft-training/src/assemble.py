#!/usr/bin/env python3
"""Merge the raw pf_*.pt dumps from extract_hidden.py into one feature file per
sequence, plus one shared prompt-prefix file per mode.

Output layout (FEAT dir):
  prefix_<mode>.pt : {"pos0": int, "ids": int32[n], "h": bf16[n, 25600]}
  seq/<rid>.pt     : {"pos0": int, "ids": int32[n], "h": bf16[n, 25600]}
  index.json       : per-sequence metadata + integrity flags

`pos0` is the ABSOLUTE token position of row 0 -- RoPE at train time must use
absolute positions (CONDITIONING.md §4), so this is load-bearing, not cosmetic.
"""
from __future__ import annotations
import argparse, glob, json, os, collections
import torch


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dump-dir", default="/mnt/dflash-dump")
    ap.add_argument("--feat-dir", default="/mnt/dflash-feats")
    ap.add_argument("--manifest", default="manifest.jsonl")
    ap.add_argument("--keep-raw", action="store_true")
    args = ap.parse_args()

    os.makedirs(os.path.join(args.feat_dir, "seq"), exist_ok=True)
    man = [json.loads(l) for l in open(os.path.join(args.dump_dir, args.manifest)) if l.strip()]
    on_disk = {os.path.basename(f) for f in glob.glob(os.path.join(args.dump_dir, "pf_*.pt"))}
    claimed = set()
    for e in man:
        claimed |= set(e.get("files") or [])
    print(f"[assemble] {len(on_disk)} raw shards on disk, {len(man)} manifest entries, "
          f"{len(claimed)} claimed by the manifest")
    orphans = sorted(on_disk - claimed)

    index = []
    for ei, e in enumerate(man):
        files = [os.path.join(args.dump_dir, f) for f in (e.get("files") or [])
                 if f in on_disk]
        if not files:
            continue
        parts = []
        for f in files:
            d = torch.load(f, map_location="cpu", weights_only=True)
            parts.append((d["pos"], d["h"], f))
        if (ei + 1) % 100 == 0:
            print(f"  assembled {ei+1}/{len(man)}", flush=True)
        parts.sort(key=lambda t: int(t[0][0].item()))
        pos = torch.cat([p for p, _, _ in parts])
        h = torch.cat([hh for _, hh, _ in parts])
        order = torch.argsort(pos)
        pos, h = pos[order], h[order]
        keep = torch.ones_like(pos, dtype=torch.bool)
        keep[1:] = pos[1:] != pos[:-1]
        pos, h = pos[keep], h[keep]
        pos0 = int(pos[0].item())
        contiguous = bool(torch.all(pos == torch.arange(pos0, pos0 + pos.numel())))

        if e["kind"] == "prefix":
            ids = torch.tensor(e["ids"], dtype=torch.int32)
            out = os.path.join(args.feat_dir, f"prefix_{e['mode']}.pt")
            name = f"prefix_{e['mode']}"
            expect0 = e["keep_from"]
        else:
            ids = torch.tensor(e["ids"], dtype=torch.int32)
            out = os.path.join(args.feat_dir, "seq", f"{e['rid']}.pt")
            name = e["rid"]
            expect0 = e["shared_len"]

        n = min(len(ids), h.shape[0])
        ok_align = (pos0 == expect0) and (len(ids) == h.shape[0])
        torch.save({"pos0": pos0, "ids": ids[:n].clone(), "h": h[:n].clone()}, out)
        index.append({
            "name": name, "kind": e["kind"], "mode": e["mode"],
            "family": e.get("family"), "query": e.get("query"),
            "pos0": pos0, "n": int(n), "expect_pos0": expect0,
            "n_ids": len(ids), "n_h": int(h.shape[0]),
            "contiguous": contiguous, "aligned": ok_align,
            "prompt_len": e.get("prompt_len"), "total_len": e.get("total_len"),
        })
        if not args.keep_raw:
            for _, _, f in parts:
                os.remove(f)

    json.dump(index, open(os.path.join(args.feat_dir, "index.json"), "w"), indent=1)
    good = [i for i in index if i["contiguous"] and i["aligned"]]
    print(f"[assemble] wrote {len(index)} feature files "
          f"({len(good)} clean, {len(index)-len(good)} with gaps/misalignment, "
          f"{len(orphans)} unclaimed shard files)")
    for i in index:
        if not (i["contiguous"] and i["aligned"]):
            print(f"  !! {i['name']}: pos0={i['pos0']} expect={i['expect_pos0']} "
                  f"n_ids={i['n_ids']} n_h={i['n_h']} contiguous={i['contiguous']}")


if __name__ == "__main__":
    main()
