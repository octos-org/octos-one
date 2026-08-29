#!/usr/bin/env python3
"""The headline comparison: cards written WITHOUT the axis vocabulary against
cards written WITH it, both rendered on the same build, judged blind.

Pure model calls over screenshots already on disk — no device, no generation,
so it runs alongside anything else.
"""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import batch_styles as B

BASE = Path(__file__).parent
OUT = BASE / "out"
done = {}
res = BASE / "cumulative.jsonl"
if res.exists():
    done = {json.loads(l)["id"] for l in open(res)}

recipes = [json.loads(l) for l in open(BASE / "recipes.jsonl")]
for r in recipes:
    sid = r["id"]
    if sid in done:
        continue
    mock, old, new = (OUT / f"{sid}-mockup.png", OUT / f"{sid}-card-noaxes.png",
                      OUT / f"{sid}-card-prev.png")
    if not (mock.exists() and old.exists() and new.exists()):
        continue
    try:
        swap = (int(sid[1:4]) % 2 == 0)
        verdict, why = B.judge_pair(mock, old, new, swap)
    except Exception as e:                      # noqa: BLE001
        print(f"[{sid}] ERR {e}", flush=True)
        continue
    with open(res, "a") as f:
        f.write(json.dumps({"id": sid, "school": r["school"], "domain": r["domain"],
                            "axes_win": verdict, "why": why}) + "\n")
    print(f"[{sid}] {'AXES' if verdict>0 else ('NO-AXES' if verdict<0 else 'tie')}: {why[:60]}", flush=True)
print("cumulative judging complete", flush=True)
