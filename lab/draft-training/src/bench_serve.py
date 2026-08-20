#!/usr/bin/env python3
"""End-to-end tok/s: production NGRAM (:30878) vs the trained DFLASH draft (:30880).

Uses HELD-OUT queries only, the real 172KB all-apps prompt, temp 0, and warms
each server first so the 53k prefill is prefix-cached and we are timing decode.
Reports decode tok/s = completion_tokens / (elapsed - measured prefill).
"""
import json, os, sys, time, urllib.request, statistics as st, collections

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_hidden import load_base, messages_for

BASE = "/home/ubuntu/qwen38-h200"


def post(port, msgs, max_tokens=4096, timeout=1200):
    """Stream so time-to-first-token can be separated from decode.

    Total wall time is contaminated by prefill: the 53k prompt is only
    prefix-cached once per prompt SHAPE (pick / compose / general each have a
    different prefix), so whichever query goes first in a shape pays a full
    prefill. Decode rate = (tokens - 1) / (total - ttft) is unaffected.
    """
    body = {"model": "x", "messages": msgs, "max_tokens": max_tokens,
            "temperature": 0, "stream": True,
            "stream_options": {"include_usage": True}}
    req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",
                                 data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    t0 = time.time()
    ttft = None
    ntok = 0
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        for raw in resp:
            line = raw.decode("utf-8", "replace").strip()
            if not line.startswith("data: "):
                continue
            payload = line[6:]
            if payload == "[DONE]":
                break
            try:
                ev = json.loads(payload)
            except Exception:
                continue
            ch = ev.get("choices") or []
            if ch and (ch[0].get("delta") or {}).get("content"):
                if ttft is None:
                    ttft = time.time() - t0
            if ev.get("usage"):
                ntok = ev["usage"].get("completion_tokens") or ntok
    dt = time.time() - t0
    if ttft is None:
        ttft = dt
    dec = max(1e-6, dt - ttft)
    return dt, ntok, ttft, dec


def main():
    # Two 27B servers cannot both keep prefill headroom on one 96GB card, so the
    # two arms are benchmarked one at a time against the same held-out queries at
    # temperature 0. Results are merged from the two runs.
    label = sys.argv[1]
    ports = {label: int(sys.argv[2])}
    base = load_base()
    splits = json.load(open(os.path.join(BASE, "draft-training", "splits", "slices.json")))
    recs = {json.loads(l)["id"]: json.loads(l)
            for l in open(os.path.join(BASE, "harvest", "out.jsonl")) if l.strip()}
    want = {"unseen_combos": 8, "cards": 6, "general": 4}
    picked = collections.defaultdict(list)
    for rid, lab in splits.items():
        if rid in recs and len(picked[lab]) < want.get(lab, 0):
            picked[lab].append(recs[rid])
    jobs = [(lab, r) for lab in want for r in picked[lab]]
    print(f"benchmark: {len(jobs)} held-out queries x {len(ports)} servers")

    # warm every prompt SHAPE so no benchmark query pays a cold 53k prefill
    for mode, q in (("pick", "weather in Berlin"),
                    ("compose", "a travel page for Rome: current weather, top things to do, and how to get around"),
                    ("general", "Write a haiku about GPUs.")):
        for name, p in ports.items():
            t, n, ttft, _ = post(p, messages_for(base, q, mode), max_tokens=16)
            print(f"  warmup {name}/{mode}: total {t:.1f}s ttft {ttft:.1f}s")

    res_path = os.path.join(BASE, "draft-training", "bench_raw.json")
    prev = json.load(open(res_path)) if os.path.exists(res_path) else {}
    out = collections.defaultdict(lambda: collections.defaultdict(list))
    for i, (lab, r) in enumerate(jobs):
        msgs = messages_for(base, r["query"], r["mode"])
        for name, p in ports.items():
            dt, n, ttft, dec = post(p, msgs, max_tokens=4096)
            if n > 1:
                out[lab][name].append((n, dt, (n - 1) / dec, ttft, dec))
        if not out[lab][label]:
            continue
        c = out[lab][label][-1]
        print(f"  [{i+1}/{len(jobs)}] {lab:14s} {r['query'][:40]!r:44s} "
              f"{c[0]:5d} tok  ttft {c[3]:5.2f}s  decode {c[4]:6.1f}s = "
              f"{c[2]:6.1f} tok/s", flush=True)

    prev[label] = {lab: [list(x) for x in out[lab][label]] for lab in want}
    json.dump(prev, open(res_path, "w"), indent=1)
    print(f"\n=== {label}: mean tok/s by slice ===")
    for lab in want:
        xs = [x[2] for x in out[lab][label]]
        if xs:
            print(f"  {lab:16s} n={len(xs):2d} {st.mean(xs):7.1f} tok/s")
    allx = [x[2] for lab in want for x in out[lab][label]]
    if allx:
        print(f"  {'ALL':16s} n={len(allx):2d} {st.mean(allx):7.1f} tok/s")
    print(f"wrote {res_path}")


if __name__ == "__main__":
    main()
