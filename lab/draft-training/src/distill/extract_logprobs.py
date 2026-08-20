#!/usr/bin/env python3
"""Extract the TARGET's soft distribution at every completion position, so the
draft can be trained by KL distillation instead of hard-label cross-entropy
(STYLIST.md section 7: the one follow-up that could change the verdict).

Why this needs a server at all: /mnt/dflash-feats stores the target's *aux*
hidden states (layers 1/16/31/46/61 of 64), which is what the draft conditions
on -- not the FINAL hidden state, so the target's logits cannot be recomputed
from them offline.  sglang will however hand back input-token logprobs for a
teacher-forced prompt, which is exactly the same teacher-forcing trick
extract_hidden.py already relies on, at one prefill per sequence.

Alignment (verified empirically against a hand-tokenized sentence):
    meta_info["input_top_logprobs"][j] is the target's top-K distribution over
    the token at absolute position (logprob_start_len + j), conditioned on
    everything before it; j == 0 is always null.
    meta_info["input_token_logprobs"][j][1] is the token actually at that
    position, which we cross-check against our own tokenization.

Output, one file per sequence:
    {"pos0": int, "ids": int32[n], "top_ids": int32[n,K], "top_lp": float32[n,K]}
  entry i is the distribution over the token at absolute position pos0+i, and
  ids[i] is the token that is actually there.

This is pure inference, so it runs against whatever healthy server is up -- no
production outage window is needed (cf. score_cards.py, which used the
production server as a judge for the same reason).
"""
from __future__ import annotations
import argparse, json, os, sys, time, threading, queue, urllib.request

import torch

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from extract_hidden import load_base, messages_for, chat_ids  # noqa: E402

BASE = "/home/ubuntu/qwen38-h200"
HARVEST = os.path.join(BASE, "harvest", "out.jsonl")


def post(url, obj, timeout=1800):
    req = urllib.request.Request(url, data=json.dumps(obj).encode(),
                                 headers={"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req, timeout=timeout))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=30878)
    ap.add_argument("--out-dir", default="/teach")
    ap.add_argument("--tokenizer", default="/models/target")
    ap.add_argument("--feat-index", default="/feats/index.json",
                    help="only extract for sequences the feature store can use")
    ap.add_argument("--topk", type=int, default=64)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--modes", default="pick,compose,general")
    ap.add_argument("--concurrency", type=int, default=2)
    ap.add_argument("--lead", type=int, default=2,
                    help="start logprobs this many positions before prompt_len")
    args = ap.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    url = f"http://127.0.0.1:{args.port}/generate"

    from transformers import AutoTokenizer
    tok = AutoTokenizer.from_pretrained(args.tokenizer, trust_remote_code=True)
    eos = tok.eos_token_id

    usable = None
    if args.feat_index and os.path.exists(args.feat_index):
        ix = json.load(open(args.feat_index))
        usable = {e["name"] for e in ix
                  if e["kind"] == "seq" and e.get("contiguous") and e.get("aligned")}
        print(f"[teach] feature store can use {len(usable)} sequences", flush=True)

    base = load_base()
    want = set(args.modes.split(","))
    recs = [json.loads(l) for l in open(HARVEST) if l.strip()]
    recs = [r for r in recs if r["mode"] in want and (r.get("content") or "").strip()]
    if usable is not None:
        recs = [r for r in recs if r["id"] in usable]
    done = {f[:-3] for f in os.listdir(args.out_dir) if f.endswith(".pt")}
    recs = [r for r in recs if r["id"] not in done]
    if args.limit:
        recs = recs[: args.limit]
    print(f"[teach] {len(recs)} to extract ({len(done)} already present), "
          f"K={args.topk}", flush=True)

    q: "queue.Queue" = queue.Queue()
    for r in recs:
        q.put(r)
    lock = threading.Lock()
    stats = {"n": 0, "bad_tok": 0, "err": 0, "pos": 0, "t0": time.time()}

    def worker():
        while True:
            try:
                r = q.get_nowait()
            except queue.Empty:
                return
            try:
                ids = chat_ids(tok, messages_for(base, r["query"], r["mode"]))
                plen = len(ids)
                cids = tok(r["content"], add_special_tokens=False)["input_ids"]
                if eos is not None:
                    cids = cids + [eos]
                ids = ids + cids
                start = max(0, plen - args.lead)
                resp = post(url, {"input_ids": ids,
                                  "sampling_params": {"max_new_tokens": 1,
                                                      "temperature": 0},
                                  "return_logprob": True,
                                  "logprob_start_len": start,
                                  "top_logprobs_num": args.topk})
                mi = resp["meta_info"]
                itl = mi["input_token_logprobs"]
                itp = mi["input_top_logprobs"]
                # j == 0 carries no distribution; keep positions start+1 ..
                n = len(itl) - 1
                if n <= 0 or len(itp) != len(itl):
                    raise ValueError(f"short logprob reply n={n}")
                pos0 = start + 1
                tids = torch.zeros((n, args.topk), dtype=torch.int32)
                tlp = torch.full((n, args.topk), -1e30, dtype=torch.float32)
                seq_ids = torch.zeros(n, dtype=torch.int32)
                mism = 0
                for i in range(n):
                    e = itl[i + 1]
                    seq_ids[i] = int(e[1])
                    if int(e[1]) != ids[pos0 + i]:
                        mism += 1
                    row = itp[i + 1] or []
                    for c, ent in enumerate(row[: args.topk]):
                        tlp[i, c] = float(ent[0])
                        tids[i, c] = int(ent[1])
                torch.save({"pos0": pos0, "ids": seq_ids, "top_ids": tids,
                            "top_lp": tlp, "prompt_len": plen,
                            "total_len": len(ids), "mode": r["mode"]},
                           os.path.join(args.out_dir, f"{r['id']}.pt"))
                with lock:
                    stats["n"] += 1
                    stats["pos"] += n
                    stats["bad_tok"] += mism
                    k = stats["n"]
                    if k % 25 == 0:
                        el = time.time() - stats["t0"]
                        print(f"[teach] {k}/{len(recs)} {el:.0f}s "
                              f"{el/k:.2f}s/req eta {(len(recs)-k)*el/k/60:.1f}min "
                              f"positions {stats['pos']} tokmismatch {stats['bad_tok']} "
                              f"err {stats['err']}", flush=True)
            except Exception as e:
                with lock:
                    stats["err"] += 1
                    if stats["err"] <= 10:
                        print(f"  !! {r['id']}: {type(e).__name__} {e}", flush=True)
            finally:
                q.task_done()

    ts = [threading.Thread(target=worker, daemon=True) for _ in range(args.concurrency)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    el = time.time() - stats["t0"]
    print(f"[teach] DONE {stats['n']} seqs, {stats['pos']} positions, "
          f"{stats['err']} errors, {stats['bad_tok']} token mismatches, {el:.0f}s",
          flush=True)


if __name__ == "__main__":
    main()
