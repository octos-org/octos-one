#!/usr/bin/env python3
"""Step 4: run one experimental ARM over the held-out query set.

An arm = (server, lenient config). The lenient config is written to the control
file the patched worker polls, so arms B and C all run inside a single server
launch; only arm A needs its own launch because the draft weights differ.

  A   card draft   + exact verify   -- baseline outputs
  B   stylist draft + exact verify  -- MUST be byte-identical to A (the physics
                                       control: exact verify is target-defined)
  C*  stylist draft + lenient verify -- the only arm allowed to differ

Outputs one JSON per arm with the full text, token counts and decode rate.
"""
from __future__ import annotations
import argparse, json, os, sys, time, urllib.request

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from extract_hidden import load_base, messages_for      # noqa: E402

BASE = "/home/ubuntu/qwen38-h200"
OUT = os.path.join(BASE, "draft-training", "stylist")


def stream(port, msgs, max_tokens=4096, timeout=1800):
    body = {"model": "x", "messages": msgs, "max_tokens": max_tokens,
            "temperature": 0, "stream": True,
            "stream_options": {"include_usage": True}}
    req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",
                                 data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    t0 = time.time(); ttft = None; parts = []; ntok = 0; finish = None
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        for raw in resp:
            line = raw.decode("utf-8", "replace").strip()
            if not line.startswith("data: "):
                continue
            p = line[6:]
            if p == "[DONE]":
                break
            try:
                ev = json.loads(p)
            except Exception:
                continue
            ch = ev.get("choices") or []
            if ch:
                d = ch[0].get("delta") or {}
                if d.get("content"):
                    if ttft is None:
                        ttft = time.time() - t0
                    parts.append(d["content"])
                if ch[0].get("finish_reason"):
                    finish = ch[0]["finish_reason"]
            if ev.get("usage"):
                ntok = ev["usage"].get("completion_tokens") or ntok
    dt = time.time() - t0
    if ttft is None:
        ttft = dt
    dec = max(1e-6, dt - ttft)
    return {"text": "".join(parts), "tokens": ntok, "ttft": round(ttft, 3),
            "decode_s": round(dec, 3), "total_s": round(dt, 3),
            "tok_s": round((ntok - 1) / dec, 2) if ntok > 1 else 0.0,
            "finish": finish}


def pick_queries(n_combo=24, n_card=16):
    slices = json.load(open(os.path.join(BASE, "draft-training", "splits", "slices.json")))
    recs = {}
    for l in open(os.path.join(BASE, "harvest", "out.jsonl")):
        if l.strip():
            r = json.loads(l)
            recs[r["id"]] = r
    got = {"unseen_combos": [], "cards": []}
    for rid in sorted(slices):
        lab = slices[rid]
        if lab in got and rid in recs and (recs[rid].get("content") or "").strip():
            got[lab].append(recs[rid])
    # de-duplicate identical query strings (the DASHBOARDS templates repeat)
    out, seen = [], set()
    for lab, want in (("unseen_combos", n_combo), ("cards", n_card)):
        for r in got[lab]:
            if len(out) >= n_combo + n_card and lab == "cards":
                break
            if r["query"] in seen:
                continue
            seen.add(r["query"])
            out.append((lab, r))
            if sum(1 for l, _ in out if l == lab) >= want:
                break
    return out


def set_ctrl(path, cfg, settle=1.5):
    tmp = path + ".tmp"
    json.dump(cfg, open(tmp, "w"))
    os.replace(tmp, path)
    time.sleep(settle)          # the worker polls at 4 Hz


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arm", required=True, help="arm name, e.g. A_exact_card")
    ap.add_argument("--port", type=int, default=30880)
    ap.add_argument("--ctrl", default="/mnt/stylist-ctrl/lenient.json")
    ap.add_argument("--mode", default="exact", choices=("exact", "topk", "tau"))
    ap.add_argument("--k", type=int, default=0)
    ap.add_argument("--tau", type=float, default=0.0)
    ap.add_argument("--protect-eos", type=int, default=1)
    ap.add_argument("--stats", type=int, default=0)
    ap.add_argument("--n-combo", type=int, default=24)
    ap.add_argument("--n-card", type=int, default=16)
    ap.add_argument("--max-tokens", type=int, default=4096)
    ap.add_argument("--no-ctrl", action="store_true",
                    help="server has no lenient patch (stock worker)")
    args = ap.parse_args()
    os.makedirs(OUT, exist_ok=True)

    cfg = {"mode": args.mode, "k": args.k, "tau": args.tau,
           "protect_eos": bool(args.protect_eos), "stats": bool(args.stats),
           "tag": args.arm}
    if not args.no_ctrl:
        set_ctrl(args.ctrl, cfg)
        print(f"[arm {args.arm}] control = {cfg}")

    jobs = pick_queries(args.n_combo, args.n_card)
    print(f"[arm {args.arm}] {len(jobs)} held-out queries")
    base = load_base()

    # warm every prompt SHAPE so no measured query pays a cold 53k prefill and
    # every arm sees the same radix-cache state
    for mode, q in (("pick", "weather in Berlin"),
                    ("compose", "a travel page for Rome: current weather, "
                                "top things to do, and how to get around")):
        w = stream(args.port, messages_for(base, q, mode), max_tokens=16)
        print(f"  warm {mode}: ttft {w['ttft']}s")

    rows = []
    t0 = time.time()
    for i, (lab, r) in enumerate(jobs):
        msgs = messages_for(base, r["query"], r["mode"])
        res = stream(args.port, msgs, max_tokens=args.max_tokens)
        res.update(id=r["id"], slice=lab, mode=r["mode"], family=r["family"],
                   query=r["query"])
        rows.append(res)
        print(f"  [{i+1}/{len(jobs)}] {lab:14s} {r['query'][:38]!r:42s} "
              f"{res['tokens']:5d} tok  {res['tok_s']:7.1f} tok/s  {res['finish']}",
              flush=True)
    p = os.path.join(OUT, f"arm_{args.arm}.json")
    json.dump({"arm": args.arm, "cfg": cfg, "port": args.port,
               "elapsed_s": round(time.time() - t0, 1), "rows": rows},
              open(p, "w"), indent=1)
    tk = [r["tok_s"] for r in rows if r["tokens"] > 1]
    print(f"[arm {args.arm}] wrote {p}: {len(rows)} rows, "
          f"mean {sum(tk)/max(1,len(tk)):.1f} tok/s, {time.time()-t0:.0f}s")


if __name__ == "__main__":
    main()
