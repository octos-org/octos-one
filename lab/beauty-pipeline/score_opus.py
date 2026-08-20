#!/usr/bin/env python3
"""Judge card screenshots with Claude Opus 5 (headless claude -p, vision via Read).

Batched: one claude invocation reads and scores BATCH screenshots, amortizing
process startup. Resume-safe via scores.jsonl. Polls shots/ until RENDER_DONE
exists and everything is scored, so it runs concurrently with render_loop.py.
"""
import json
import subprocess
import time
from pathlib import Path

BASE = Path(__file__).resolve().parent
BATCH = 6

HEADER = """You are a demanding design director with impeccable visual taste (美学),
judging generated mini-app phone cards. For EACH screenshot listed below, Read the
file and judge its VISUAL DESIGN only — not data correctness.

Return ONE JSON array and nothing else, one object per file, same order as listed:
{"file": "<basename without .png>", "valid": true|false, "overall": 1-10,
 "hierarchy": 1-10, "spacing": 1-10, "color": 1-10, "typography": 1-10,
 "imagery": 1-10, "density": 1-10, "notes": "<=25 words, biggest strength or flaw"}

valid=false if blank, an error message, or visually broken (overlap/clipping/unreadable).
overall: 10 = premium and cinematic, 5 = acceptable but plain, 2 = ugly or cluttered.
Judge each screenshot independently and be consistent across the whole set.

Files:
"""


def batch_score(items):
    lines = [f"{i + 1}. {BASE / 'shots' / (cid + '.png')}  (user request: \"{q[:80]}\")"
             for i, (cid, q) in enumerate(items)]
    prompt = HEADER + "\n".join(lines)
    r = subprocess.run(
        ["claude", "-p", prompt, "--model", "opus", "--allowedTools", "Read",
         "--output-format", "json", "--max-turns", "40"],
        capture_output=True, text=True, timeout=900, cwd=BASE)
    out = json.loads(r.stdout)["result"]
    arr = json.loads(out[out.find("["):out.rfind("]") + 1])
    if not isinstance(arr, list):
        raise ValueError("judge did not return a list")
    return arr


def main():
    meta = {m["id"]: m for m in map(json.loads, open(BASE / "meta.jsonl"))}
    scores_path = BASE / "scores.jsonl"
    scored = set()
    if scores_path.exists():
        scored = {json.loads(l)["id"] for l in open(scores_path)}
    print(f"opus judge starting; {len(scored)} already scored", flush=True)

    while True:
        pending = [p.stem for p in sorted((BASE / "shots").glob("*.png"))
                   if p.stem not in scored]
        for i in range(0, len(pending), BATCH):
            chunk = [(cid, meta[cid]["query"]) for cid in pending[i:i + BATCH]]
            try:
                results = batch_score(chunk)
            except Exception as e:  # noqa: BLE001 — timeout/parse/ratelimit all retry next pass
                print(f"ERR  batch {chunk[0][0]}..: {e}", flush=True)
                time.sleep(20)
                continue
            wanted = {cid for cid, _ in chunk}
            with open(scores_path, "a") as f:
                for s in results:
                    cid = str(s.get("file", "")).removesuffix(".png")
                    if cid not in wanted:
                        continue
                    s["id"] = cid
                    s["model"] = "opus-5"
                    f.write(json.dumps(s) + "\n")
                    scored.add(cid)
                    print(f"{s.get('overall', '?'):>2}  {cid}  {str(s.get('notes', ''))[:70]}",
                          flush=True)
        if (BASE / "RENDER_DONE").exists() and not [
                p for p in (BASE / "shots").glob("*.png") if p.stem not in scored]:
            break
        time.sleep(30)

    print(f"scoring complete: {len(scored)} cards scored", flush=True)
    (BASE / "SCORE_DONE").write_text("done\n")


if __name__ == "__main__":
    main()
