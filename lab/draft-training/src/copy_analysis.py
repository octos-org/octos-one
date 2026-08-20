#!/usr/bin/env python3
"""Quantify the project's thesis on the harvested data, without a GPU.

CLAUDE.md claims: L0 cards contain no live facts, so nearly every "novel" token
is COPYABLE from the request; a learned draft can learn that copying, a trie
cannot. This measures both halves.

Per anchor `a` in a generation, with block size B (so B-1 draftable slots):

  oracle_any     longest prefix of t[a+1..] that occurs contiguously anywhere in
                 t[0..a]  -- the ceiling for ANY copy-based drafter
  oracle_prompt  ... restricted to the prompt (the request itself)
  oracle_self    ... restricted to what this generation has already emitted
  ngram_suffix   what a suffix-automaton actually proposes: take the LONGEST
                 suffix of t[0..a] that occurred earlier, and draft its
                 continuation. This is the deployed NGRAM's mechanism, so it is
                 the honest baseline to beat.

The gap between `ngram_suffix` and `oracle_any` is the headroom a learned draft
can address. The gap between `oracle_any` and B-1 is what must be *generated*
rather than copied.
"""
from __future__ import annotations
import argparse, json, os, random, sys, collections
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import re
from extract_hidden import load_base, messages_for, chat_ids

SECTION_RE = re.compile(
    r"(?m)^(?:\s{0,6})(source\s|state\s|Panel\b|Card\b|Section\b|Col\(|Row\(|Header\b|Group\b|#\s*model:|```)")


def seam_token_indices(tok, cids):
    """Token offsets (relative to the completion) that start a card section."""
    offs, cur, chunks = [], 0, []
    for tid in cids:
        piece = tok.decode([tid])
        offs.append(cur); chunks.append(piece); cur += len(piece)
    text = "".join(chunks)
    out = []
    for m in SECTION_RE.finditer(text):
        c = m.start()
        lo, hi = 0, len(offs) - 1
        while lo < hi:
            mid = (lo + hi) // 2
            if offs[mid] < c:
                lo = mid + 1
            else:
                hi = mid
        out.append(lo)
    return sorted(set(out))

BASE = "/home/ubuntu/qwen38-h200"


def longest_match(ctx: np.ndarray, tgt: np.ndarray, lo: int, hi: int) -> int:
    """Longest prefix of tgt occurring contiguously in ctx[lo:hi]."""
    if hi - lo <= 0 or tgt.size == 0:
        return 0
    seg = ctx[lo:hi]
    idx = np.flatnonzero(seg == tgt[0])
    if idx.size == 0:
        return 0
    best = 1
    alive = idx
    for l in range(1, tgt.size):
        alive = alive[alive + l < seg.size]
        if alive.size == 0:
            break
        alive = alive[seg[alive + l] == tgt[l]]
        if alive.size == 0:
            break
        best = l + 1
    return best


def ngram_propose(hay: np.ndarray, cut: int, tgt: np.ndarray,
                  max_order: int = 32) -> int:
    """Longest-suffix-match continuation, then count leading agreement.

    `hay` is [corpus documents ... , SEP, context so far]; `cut` is where the
    context ends (== hay.size). Mirrors a suffix automaton: find the LONGEST
    suffix of the context that occurs earlier anywhere in `hay`, and propose
    what followed it there. Documents are separated by a sentinel so a match
    cannot span two of them. Ties break to the most recent occurrence.

    Candidates are grown by extending the suffix leftwards, so the cost is
    O(max_order * |surviving candidates|) rather than one scan per order.
    """
    n = cut
    if n < 2 or tgt.size == 0:
        return 0
    cand = np.flatnonzero(hay[: n - 1] == hay[n - 1])   # order-1 matches
    if cand.size == 0:
        return 0
    best = cand
    order = 1
    while order < max_order:
        prev = cand[(cand - order >= 0)]
        if prev.size == 0:
            break
        prev = prev[hay[prev - order] == hay[n - 1 - order]]
        if prev.size == 0:
            break
        cand, best, order = prev, prev, order + 1
        if n - 1 - order < 0:
            break
    j = int(best[-1]) + 1                                # continuation point
    k = min(tgt.size, hay.size - j)
    if k <= 0:
        return 0
    eq = hay[j: j + k] == tgt[:k]
    return int(np.argmin(eq)) if not eq.all() else int(k)


def load_corpus(path, tok, sep_id=-1):
    """Tokenize the external NGRAM corpus, sentinel-separated."""
    if not os.path.exists(path):
        return np.zeros(0, dtype=np.int64)
    parts = []
    for line in open(path):
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except Exception:
            continue
        txt = d if isinstance(d, str) else " ".join(
            v for v in d.values() if isinstance(v, str))
        if not txt:
            continue
        parts.append(np.array(tok(txt, add_special_tokens=False)["input_ids"],
                              dtype=np.int64))
        parts.append(np.array([sep_id], dtype=np.int64))
    return np.concatenate(parts) if parts else np.zeros(0, dtype=np.int64)


def k_window_accept(acc_hist, K):
    """accepted-in-a-K-window == K - verifies_used (see src/test_accounting.py)."""
    got = tot = 0
    for acc in acc_hist:
        commit = acc + 1
        if tot + commit >= K:
            return got + min(acc, K - tot - 1)
        got += acc
        tot += commit
    return None


def chain_window(t, P, a0, D, K, corpus, mode):
    """Replay the real accept loop from anchor a0 with a copy-based drafter."""
    a, hist = a0, []
    while True:
        tgt = t[a + 1: a + 1 + D]
        if tgt.size == 0:
            return None
        ctx = t[:a + 1]
        if mode == "ngram_no_prompt":
            gen = t[P:a + 1]
            hay = np.concatenate([corpus, gen]) if corpus.size else gen
            acc = ngram_propose(hay, hay.size, tgt) if hay.size > 1 else 0
        elif mode == "ngram_suffix":
            hay = np.concatenate([corpus, ctx]) if corpus.size else ctx
            acc = ngram_propose(hay, hay.size, tgt)
        else:  # oracle_any
            acc = longest_match(ctx, tgt, 0, ctx.size)
        hist.append(int(acc))
        a += acc + 1
        v = k_window_accept(hist, K)
        if v is not None:
            return v
        if a + D + 1 >= t.size or len(hist) > 4 * K:
            return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tokenizer", default="/models/target")
    ap.add_argument("--harvest", default=os.path.join(BASE, "harvest", "out.jsonl"))
    ap.add_argument("--block-size", type=int, default=16)
    ap.add_argument("--per-seq", type=int, default=25)
    ap.add_argument("--max-seq-per-mode", type=int, default=60)
    ap.add_argument("--search-window", type=int, default=8192,
                    help="how far back the copy search may look (0 = whole sequence)")
    ap.add_argument("--corpus", default=os.path.join(BASE, "ngram-corpus", "cards.jsonl"))
    ap.add_argument("--seams", action="store_true",
                    help="restrict anchors to +-R tokens around section boundaries")
    ap.add_argument("--seam-radius", type=int, default=10)
    ap.add_argument("--chain", action="store_true",
                    help="also run the chained K-window simulation so the numbers "
                         "are in the same units as the Goal-5 gates")
    ap.add_argument("--ks", default="8,16,32,48")
    ap.add_argument("--only-ids", default=None,
                    help="json list of harvest ids to restrict to (e.g. the holdout), "
                         "so the incumbent is measured on exactly the sequences the "
                         "trained draft is evaluated on")
    ap.add_argument("--slices", default=None,
                    help="json map id -> slice label; reports per gate slice")
    ap.add_argument("--out", default=None)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()

    from transformers import AutoTokenizer
    tok = AutoTokenizer.from_pretrained(args.tokenizer, trust_remote_code=True)
    base = load_base()
    recs = [json.loads(l) for l in open(args.harvest) if l.strip()]
    recs = [r for r in recs if (r.get("content") or "").strip()]
    if args.only_ids:
        keep = set(json.load(open(args.only_ids)))
        recs = [r for r in recs if r["id"] in keep]
    slice_map = json.load(open(args.slices)) if args.slices else {}
    bym = collections.defaultdict(list)
    for r in recs:
        bym[r["mode"]].append(r)
        lab = slice_map.get(r["id"])
        if lab:
            bym[f"GATE:{lab}"].append(r)

    corpus = load_corpus(args.corpus, tok)
    print(f"NGRAM corpus: {corpus.size} tokens from {args.corpus}")
    rng = random.Random(args.seed)
    D = args.block_size - 1
    out = {}
    for mode, rs in sorted(bym.items()):
        rs = rs[: args.max_seq_per_mode]
        acc = collections.defaultdict(list)
        chain_acc = collections.defaultdict(list)
        for r in rs:
            pids = chat_ids(tok, messages_for(base, r["query"], r["mode"]))
            cids = tok(r["content"], add_special_tokens=False)["input_ids"]
            t = np.array(pids + cids, dtype=np.int64)
            P, N = len(pids), len(t)
            if N - P <= D + 2:
                continue
            lo_a, hi_a = P, N - D - 1
            if args.seams:
                R = args.seam_radius
                cand = sorted({P + j + d for j in seam_token_indices(tok, cids)
                               for d in range(-R, R + 1)
                               if lo_a <= P + j + d < hi_a})
                if not cand:
                    continue
                anchors = sorted(rng.sample(cand, min(args.per_seq, len(cand))))
            else:
                anchors = sorted(rng.sample(range(lo_a, hi_a),
                                            min(args.per_seq, hi_a - lo_a)))
            for a in anchors:
                tgt = t[a + 1: a + 1 + D]
                w = args.search_window or a + 1
                start = max(0, a + 1 - w)
                ctx = t[start: a + 1]
                pcut = max(0, P - start)          # prompt part of ctx is [0, pcut)
                acc["oracle_any"].append(longest_match(ctx, tgt, 0, ctx.size))
                acc["oracle_prompt"].append(longest_match(ctx, tgt, 0, pcut))
                acc["oracle_self"].append(longest_match(ctx, tgt, pcut, ctx.size))
                acc["oracle_corpus"].append(
                    longest_match(corpus, tgt, 0, corpus.size) if corpus.size else 0)
                hay = np.concatenate([corpus, ctx]) if corpus.size else ctx
                acc["ngram_suffix"].append(ngram_propose(hay, hay.size, tgt))
                # what a drafter that does NOT index the request can reach:
                # the external corpus plus this generation's own output
                gen = ctx[pcut:]
                hay2 = np.concatenate([corpus, gen]) if corpus.size else gen
                acc["ngram_no_prompt"].append(
                    ngram_propose(hay2, hay2.size, tgt) if hay2.size > 1 else 0)
                if args.chain:
                    for K in [int(x) for x in args.ks.split(",")]:
                        for mm in ("ngram_no_prompt", "ngram_suffix", "oracle_any"):
                            v = chain_window(t, P, a, D, K, corpus, mm)
                            if v is not None:
                                chain_acc[f"{mm}@{K}"].append(v)
                acc["oracle_corpus_self"].append(
                    max(longest_match(corpus, tgt, 0, corpus.size) if corpus.size else 0,
                        longest_match(gen, tgt, 0, gen.size)))
        n = len(acc["oracle_any"])
        row = {"n_anchors": n, "n_seqs": len(rs), "draftable_slots": D}
        for k, v in acc.items():
            v = np.array(v)
            row[k] = {"mean": float(v.mean()), "p50": float(np.median(v)),
                      "frac_full": float((v >= D).mean()),
                      "frac_zero": float((v == 0).mean())}
        if args.chain:
            row["chain"] = {k: float(np.mean(v)) for k, v in sorted(chain_acc.items())}
        out[mode] = row
        print(f"\n[{mode}] {len(rs)} seqs, {n} anchors, {D} draftable slots/block")
        for k in ("ngram_no_prompt", "oracle_corpus_self", "ngram_suffix",
                  "oracle_corpus", "oracle_self", "oracle_prompt", "oracle_any"):
            d = row[k]
            print(f"  {k:18s} mean {d['mean']:5.2f}/{D}  p50 {d['p50']:4.1f}  "
                  f"full-block {100*d['frac_full']:5.1f}%  zero {100*d['frac_zero']:5.1f}%")
        if args.chain:
            print("  chained K-window (accepted of K, ceiling K - ceil(K/B)):")
            for k, v in sorted(row["chain"].items()):
                print(f"    {k:26s} {v:6.2f}")
    if args.out:
        json.dump(out, open(args.out, "w"), indent=1)
        print(f"\nwrote {args.out}")


if __name__ == "__main__":
    main()
