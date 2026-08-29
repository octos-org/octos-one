#!/usr/bin/env python3
"""How much of my paired-judging signal is the judge talking to itself?

Re-judges the SAME 100 pairs as cumulative.jsonl with the order FLIPPED. A
judge that perceives a real difference returns the same winner either way. A
judge that is guessing flips ~50% of the time.

This calibrates every paired number this project has produced. Without it a
61-39 is uninterpretable: it could be a real effect or it could be one draw
from a very wide distribution.
"""
import json, sys, math
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import batch_styles as B

BASE = Path(__file__).parent
OUT = BASE / "out"
res = BASE / "noise_floor.jsonl"
done = {json.loads(l)["id"] for l in open(res)} if res.exists() else set()

first = {json.loads(l)["id"]: json.loads(l) for l in open(BASE / "cumulative.jsonl")}

for sid, rec in first.items():
    if sid in done:
        continue
    mock, old, new = (OUT / f"{sid}-mockup.png", OUT / f"{sid}-card-noaxes.png",
                      OUT / f"{sid}-card-prev.png")
    if not (mock.exists() and old.exists() and new.exists()):
        continue
    try:
        # the OPPOSITE order to the first pass
        swap = not (int(sid[1:4]) % 2 == 0)
        verdict, why = B.judge_pair(mock, old, new, swap)
    except Exception as e:                      # noqa: BLE001
        print(f"[{sid}] ERR {e}", flush=True)
        continue
    agree = (verdict > 0) == (rec["axes_win"] > 0)
    with open(res, "a") as f:
        f.write(json.dumps({"id": sid, "school": rec["school"],
                            "pass1": rec["axes_win"], "pass2": verdict,
                            "agree": agree, "why": why}) + "\n")
    print(f"[{sid}] {'AGREE' if agree else 'FLIP '} p1={rec['axes_win']:+d} p2={verdict:+d}", flush=True)

rows = [json.loads(l) for l in open(res)]
n = len(rows)
a = sum(1 for r in rows if r["agree"])
print(f"\nself-agreement {a}/{n} = {100*a/max(n,1):.0f}%", flush=True)
if n:
    # A judge with true-effect probability p on a real difference agrees at
    # p^2+(1-p)^2. Invert to get the implied per-call reliability.
    frac = a / n
    disc = max(0.0, 2 * frac - 1)
    p = 0.5 * (1 + math.sqrt(disc))
    print(f"implied per-call reliability ~{100*p:.0f}%", flush=True)
    print(f"=> a single-judgment tally of N=100 has a noise sd of about "
          f"{100*math.sqrt(0.25*(1-disc))/10:.1f} wins", flush=True)
