#!/usr/bin/env python3
"""Extract target aux hidden states (the DFlash context features) for every
harvested generation, by teacher-forcing prompt+completion through a DFLASH
sglang server whose worker has been instrumented by src/instrument/patch_worker.py.

Runs INSIDE the extraction container (needs the tokenizer + torch).

Why teacher forcing is exact, not an approximation: at serve time the draft's
context KV is always built from the target's hidden states for the tokens
actually committed to the sequence (CONDITIONING.md §5). Replaying the recorded
temp-0 output as a prompt reproduces those tokens, so a single prefill with
CaptureHiddenMode.FULL yields exactly the features the draft would have seen at
every position -- and it costs one prefill instead of ~110 decode steps.

Requests are issued one at a time and the worker writes its dumps synchronously
inside the prefill, so a before/after listing of the dump dir attributes shard
files to requests exactly -- no dependence on sglang echoing a request id back.

Phases:
  A  floor=0, one request per prompt shape -> capture the shared 53k prompt once
  B  floor=<shared prefix len>, one request per record -> capture only the
     per-request tail (query tokens + the whole completion)
"""
from __future__ import annotations
import argparse, json, os, sys, time, urllib.request, collections

BASE = "/home/ubuntu/qwen38-h200"
HARVEST = os.path.join(BASE, "harvest", "out.jsonl")
WARM = os.path.join(BASE, "warm_request.json")

PICK_SENTINEL = "PICK the ONE app that answers the user request, then write THAT app's card"
COMPOSE_TEXT = ("COMPOSE one card that answers the user request, drawing sections from AS MANY "
    "of the APP sections below as the request spans (weather panes, event/activity lists, "
    "nav routes, video rows, stock tiles, news feeds...). Declare each section's sources per "
    "its own app spec, share common state (like the place) across sections, and keep every "
    "L0 rule. If the request spans one domain only, a single-app card is correct")


def load_base():
    b = json.load(open(WARM))
    b.pop("tools", None); b.pop("tool_choice", None)
    return b


def messages_for(base, query, mode):
    """Byte-identical to harvest.py make_request / make_general."""
    if mode == "general":
        return [{"role": "user", "content": query}]
    d = json.loads(json.dumps(base))
    for m in d["messages"]:
        c = m.get("content")
        if isinstance(c, str) and "weather in Berlin" in c:
            c = c.replace("weather in Berlin", query)
            if mode == "compose":
                c = c.replace(PICK_SENTINEL, COMPOSE_TEXT)
                c = c.replace("for the ONE app you picked", "for the composition")
            m["content"] = c
    return d["messages"]


def chat_ids(tok, msgs):
    """transformers 5.x apply_chat_template returns a dict by default; taking
    len() of that silently yields 2 instead of the token count."""
    out = tok.apply_chat_template(msgs, add_generation_prompt=True, tokenize=True,
                                  enable_thinking=False)
    if hasattr(out, "keys"):          # transformers 5.x returns a BatchEncoding,
        out = out["input_ids"]        # which is a UserDict -- not a `dict`, and
                                      # len() of it is 2 (the number of keys)
    if out and isinstance(out[0], list):
        out = out[0]
    return list(out)


def post(url, obj, timeout=1800):
    req = urllib.request.Request(url, data=json.dumps(obj).encode(),
                                 headers={"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req, timeout=timeout))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=30879)
    ap.add_argument("--dump-dir", default="/dump")
    ap.add_argument("--out-manifest", default="/dump/manifest.jsonl")
    ap.add_argument("--tokenizer", default="/models/target")
    ap.add_argument("--limit", type=int, default=0, help="0 = all records")
    ap.add_argument("--modes", default="pick,compose,general")
    ap.add_argument("--window", type=int, default=4096,
                    help="prompt-tail positions to keep for the shared prefix")
    ap.add_argument("--phase", default="ab", choices=["a", "b", "ab"])
    ap.add_argument("--truncate-frac", type=float, default=0.0,
                    help="cut each completion at this fraction so the single decode "
                         "step lands mid-card (parity probing, not training data)")
    args = ap.parse_args()

    from transformers import AutoTokenizer
    tok = AutoTokenizer.from_pretrained(args.tokenizer, trust_remote_code=True)
    eos = tok.eos_token_id
    url = f"http://127.0.0.1:{args.port}/generate"
    floor_path = os.path.join(args.dump_dir, "floor.txt")
    os.makedirs(args.dump_dir, exist_ok=True)

    def snapshot():
        return {f for f in os.listdir(args.dump_dir) if f.startswith("pf_")}

    def set_floor(v):
        tmp = floor_path + ".tmp"
        open(tmp, "w").write(str(int(v)))
        os.replace(tmp, floor_path)

    base = load_base()
    want_modes = set(args.modes.split(","))
    recs = [json.loads(l) for l in open(HARVEST) if l.strip()]
    recs = [r for r in recs if r["mode"] in want_modes and (r.get("content") or "").strip()]
    if args.limit:
        # keep the mode mix when subsetting
        bym = collections.defaultdict(list)
        for r in recs: bym[r["mode"]].append(r)
        share = max(1, args.limit // max(1, len(bym)))
        recs = [r for m in bym for r in bym[m][:share]][: args.limit]
    print(f"[extract] {len(recs)} records, modes={sorted(want_modes)}", flush=True)

    # ---- tokenize everything up front; validate against recorded counts ----
    prompts, comps, bad = {}, {}, 0
    for r in recs:
        ids = chat_ids(tok, messages_for(base, r["query"], r["mode"]))
        cids = tok(r["content"], add_special_tokens=False)["input_ids"]
        if eos is not None:
            cids = cids + [eos]   # the served completion_tokens counts the stop token
        prompts[r["id"]] = ids
        comps[r["id"]] = cids
        if r.get("prompt_tokens") and abs(len(ids) - int(r["prompt_tokens"])) > 2:
            bad += 1
            if bad <= 5:
                print(f"  !! prompt token mismatch {r['id']}: ours={len(ids)} "
                      f"served={r['prompt_tokens']}", flush=True)
    print(f"[extract] prompt-token mismatches: {bad}/{len(recs)}", flush=True)
    ctok_delta = [len(comps[r['id']]) - int(r.get('completion_tokens') or 0) for r in recs]
    exact = sum(1 for d in ctok_delta if d == 0)
    print(f"[extract] completion retokenization exact: {exact}/{len(recs)} "
          f"(mean delta {sum(ctok_delta)/max(1,len(ctok_delta)):+.2f})", flush=True)

    # ---- shared prefix length per mode ------------------------------------
    shared = {}
    for m in sorted(want_modes):
        ms = [prompts[r["id"]] for r in recs if r["mode"] == m]
        if not ms:
            continue
        if len(ms) == 1:
            shared[m] = len(ms[0])
            continue
        n = min(len(a) for a in ms)
        lo = 0
        ref = ms[0]
        while lo < n and all(a[lo] == ref[lo] for a in ms):
            lo += 1
        shared[m] = lo
    print(f"[extract] shared prompt prefix per mode: {shared}", flush=True)

    man = open(args.out_manifest, "a")

    # ---- phase A: capture the shared prompt prefix once per mode ----------
    if "a" in args.phase:
        set_floor(0)
        for m in sorted(shared):
            ref = next(r for r in recs if r["mode"] == m)
            ids = prompts[ref["id"]][: shared[m]]
            keep_from = max(0, shared[m] - args.window)
            print(f"[extract][A] mode={m} seeding {len(ids)} prompt tokens "
                  f"(keeping >= {keep_from})", flush=True)
            set_floor(keep_from)
            t0 = time.time()
            before = snapshot()
            resp = post(url, {"input_ids": ids,
                              "sampling_params": {"max_new_tokens": 1, "temperature": 0}})
            man.write(json.dumps({"kind": "prefix", "mode": m,
                                  "rid": (resp.get("meta_info") or {}).get("id"),
                                  "files": sorted(snapshot() - before),
                                  "shared_len": shared[m], "keep_from": keep_from,
                                  "ids": ids[keep_from:]}) + "\n")
            man.flush()
            print(f"[extract][A] mode={m} done in {time.time()-t0:.1f}s", flush=True)

    # ---- phase B: per-record tails ---------------------------------------
    if "b" in args.phase:
        done = set()
        if os.path.exists(args.out_manifest):
            for l in open(args.out_manifest):
                try:
                    o = json.loads(l)
                    if o.get("kind") == "seq":
                        done.add(o["rid"])
                except Exception:
                    pass
        todo = [r for r in recs if r["id"] not in done]
        print(f"[extract][B] {len(todo)} to go ({len(done)} already done)", flush=True)
        t0 = time.time()
        for i, r in enumerate(todo):
            m = r["mode"]
            set_floor(shared[m])
            cids = comps[r["id"]]
            if args.truncate_frac:
                cut = max(8, int(len(cids) * args.truncate_frac))
                cids = cids[:cut]
            ids = prompts[r["id"]] + cids
            before = snapshot()
            try:
                resp = post(url, {"input_ids": ids,
                                  "sampling_params": {"max_new_tokens": 1, "temperature": 0}})
            except Exception as e:
                print(f"  !! {r['id']}: {type(e).__name__} {e}", flush=True)
                continue
            man.write(json.dumps({
                "kind": "seq", "rid": r["id"],
                "sgl_rid": (resp.get("meta_info") or {}).get("id"),
                "files": sorted(snapshot() - before),
                "mode": m, "family": r["family"],
                "query": r["query"], "shared_len": shared[m],
                "prompt_len": len(prompts[r["id"]]),
                "total_len": len(ids),
                "ids": ids[shared[m]:],
                "truncate_frac": args.truncate_frac,
            }) + "\n")
            man.flush()
            if (i + 1) % 25 == 0:
                el = time.time() - t0
                print(f"[extract][B] {i+1}/{len(todo)}  {el:.0f}s  "
                      f"{el/(i+1):.2f}s/req  eta {(len(todo)-i-1)*el/(i+1)/60:.1f}min",
                      flush=True)
    man.close()
    print("[extract] DONE", flush=True)


if __name__ == "__main__":
    main()
