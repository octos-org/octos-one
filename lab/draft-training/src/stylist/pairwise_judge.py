#!/usr/bin/env python3
"""Step 5: blind pairwise design judging, arm A vs arm C.

Position bias is real and large for LLM judges, so every pair is asked TWICE
with the presentation order swapped. A win only counts as a win if it survives
both orders; disagreement between the two orders is scored as a tie. That makes
the metric conservative -- it cannot manufacture a win out of position bias.

Judge is the production server (:30878): the same model that wrote both cards.
"""
from __future__ import annotations
import argparse, json, os, re, sys, threading, queue, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from score_cards import post                             # noqa: E402

BASE = "/home/ubuntu/qwen38-h200"
OUT = os.path.join(BASE, "draft-training", "stylist")

PROMPT = """Two UI cards, written in the runl0 declarative DSL, answer the same user request.
Judge DESIGN QUALITY ONLY: visual hierarchy, theme/copy coherence, imagery, section richness,
information density. Ignore factual correctness and whether the DSL would compile.

Pick the card a designer would ship. If they are genuinely equivalent in design quality,
say TIE -- but prefer to pick a winner when there is any real difference.

Reply with EXACTLY ONE line and nothing else:
VERDICT:<A|B|TIE> REASON:<at most 12 words>

USER REQUEST:
{q}

=== CARD A ===
{a}

=== CARD B ===
{b}
"""
VRE = re.compile(r"VERDICT:\s*(A|B|TIE)\b", re.I)


def ask(port, q, first, second):
    txt = post(port, [{"role": "user", "content":
                       PROMPT.format(q=q, a=first, b=second)}], max_tokens=40)
    m = VRE.search(txt)
    return (m.group(1).upper() if m else None), txt.strip()[:160]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ref", default="A_exact_card")
    ap.add_argument("--arm", required=True)
    ap.add_argument("--port", type=int, default=30878)
    ap.add_argument("--dir", default=OUT)
    ap.add_argument("--concurrency", type=int, default=3)
    ap.add_argument("--only-changed", type=int, default=1,
                    help="skip pairs whose outputs are byte-identical (auto-tie)")
    args = ap.parse_args()

    ref = json.load(open(os.path.join(args.dir, f"arm_{args.ref}.json")))
    arm = json.load(open(os.path.join(args.dir, f"arm_{args.arm}.json")))
    rmap = {r["id"]: r for r in ref["rows"]}
    pairs = [(r, rmap[r["id"]]) for r in arm["rows"] if r["id"] in rmap]
    skipped = [p for p in pairs if p[0]["text"] == p[1]["text"]]
    if args.only_changed:
        pairs = [p for p in pairs if p[0]["text"] != p[1]["text"]]
    print(f"[judge] {args.arm} vs {args.ref}: {len(pairs)} changed pairs "
          f"({len(skipped)} identical, auto-tie)")

    q = queue.Queue()
    for p in pairs:
        q.put(p)
    lock = threading.Lock()
    rows = []

    def worker():
        while True:
            try:
                c, a = q.get_nowait()
            except queue.Empty:
                return
            try:
                # order 1: reference first; order 2: candidate first
                v1, r1 = ask(args.port, a["query"], a["text"], c["text"])
                v2, r2 = ask(args.port, a["query"], c["text"], a["text"])
            except Exception as e:
                v1 = v2 = None; r1 = r2 = f"ERROR {e}"
            # translate to "did the candidate (arm) win?"
            w1 = {"A": "ref", "B": "arm", "TIE": "tie"}.get(v1)
            w2 = {"A": "arm", "B": "ref", "TIE": "tie"}.get(v2)
            if w1 == w2 and w1 is not None:
                out = w1
            elif w1 is None or w2 is None:
                out = "error"
            else:
                out = "tie"          # order-dependent => not a real preference
            with lock:
                rows.append({"id": a["id"], "slice": a["slice"], "query": a["query"],
                             "order1": w1, "order2": w2, "verdict": out,
                             "why1": r1, "why2": r2})
                if len(rows) % 10 == 0:
                    print(f"  {len(rows)}/{len(pairs)}", flush=True)

    ths = [threading.Thread(target=worker, daemon=True) for _ in range(args.concurrency)]
    t0 = time.time()
    for t in ths: t.start()
    for t in ths: t.join()

    for s in skipped:
        rows.append({"id": s[1]["id"], "slice": s[1]["slice"], "query": s[1]["query"],
                     "order1": "identical", "order2": "identical",
                     "verdict": "tie", "why1": "byte-identical", "why2": ""})
    tally = {k: sum(1 for r in rows if r["verdict"] == k)
             for k in ("arm", "ref", "tie", "error")}
    consistent = sum(1 for r in rows if r["order1"] == r["order2"]
                     and r["order1"] in ("arm", "ref"))
    flipped = sum(1 for r in rows if r["order1"] != r["order2"]
                  and r["order1"] in ("arm", "ref") and r["order2"] in ("arm", "ref"))
    res = {"arm": args.arm, "ref": args.ref, "n": len(rows), "tally": tally,
           "order_consistent_wins": consistent, "order_flipped": flipped,
           "n_identical_autotie": len(skipped), "rows": rows,
           "elapsed_s": round(time.time() - t0, 1)}
    p = os.path.join(args.dir, f"judge_{args.arm}_vs_{args.ref}.json")
    json.dump(res, open(p, "w"), indent=1)
    print(f"[judge] {args.arm} wins {tally['arm']}, {args.ref} wins {tally['ref']}, "
          f"tie {tally['tie']}, error {tally['error']}  "
          f"(order-flipped {flipped}) -> {p}")


if __name__ == "__main__":
    main()
