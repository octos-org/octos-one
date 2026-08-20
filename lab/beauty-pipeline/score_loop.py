#!/usr/bin/env python3
"""Vision-judge every rendered card screenshot against a fixed UX rubric.

Runs concurrently with render_loop.py: polls shots/ and scores whatever has
appeared, exiting once RENDER_DONE exists and everything is scored.
Resume-safe via scores.jsonl. The judge never sees the card source or the
model's name — only pixels and the user's query — so it grades what a person
would actually see.
"""
import base64
import json
import time
import urllib.request
from pathlib import Path

BASE = Path(__file__).resolve().parent
KEY = (BASE.parent / "oai_key").read_text().strip()
API = "https://api.openai.com/v1"
MODEL_PREFS = ["gpt-5-mini", "gpt-4.1-mini", "gpt-4o-mini", "gpt-4o"]

RUBRIC = """You are a strict mobile UI design judge. The image is a screenshot of a
generated mini-app card on a phone, produced for this user request: "{query}".

Score its VISUAL DESIGN quality (美学) only — not data correctness. Judge like a
demanding design director reviewing a production app.

Return STRICT JSON, nothing else:
{{"valid": bool,        // false if blank, an error message, or visually broken (overlapping/clipped text, unreadable)
  "overall": 1-10,      // 10 = premium, cinematic, cohesive; 5 = acceptable but plain; 2 = ugly or cluttered
  "hierarchy": 1-10,    // clear focal point, sensible reading order, section structure
  "spacing": 1-10,      // alignment, margins, breathing room, consistent rhythm
  "color": 1-10,        // palette cohesion, theme use, contrast
  "typography": 1-10,   // size scale, weight contrast, legibility
  "imagery": 1-10,      // photo/graphic use and integration (score 4 if none where one would help, 6 if none needed)
  "density": 1-10,      // information richness without clutter
  "notes": "<=25 words, the single biggest strength or flaw"}}"""


def call(payload):
    req = urllib.request.Request(
        f"{API}/chat/completions",
        data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


def pick_model():
    req = urllib.request.Request(f"{API}/models", headers={"Authorization": f"Bearer {KEY}"})
    with urllib.request.urlopen(req, timeout=30) as r:
        have = {m["id"] for m in json.load(r)["data"]}
    for m in MODEL_PREFS:
        if m in have:
            return m
    raise SystemExit(f"none of {MODEL_PREFS} available")


def score(model, shot_path, query):
    b64 = base64.b64encode(shot_path.read_bytes()).decode()
    payload = {
        "model": model,
        "response_format": {"type": "json_object"},
        "max_completion_tokens": 2000,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": RUBRIC.format(query=query.replace('"', "'"))},
                {"type": "image_url",
                 "image_url": {"url": f"data:image/png;base64,{b64}", "detail": "low"}},
            ],
        }],
    }
    for attempt in range(4):
        try:
            out = call(payload)
            return json.loads(out["choices"][0]["message"]["content"])
        except Exception as e:  # noqa: BLE001 — 429/5xx/truncated JSON all retry the same way
            if attempt == 3:
                raise
            time.sleep(5 * (attempt + 1))


def main():
    meta = {m["id"]: m for m in map(json.loads, open(BASE / "meta.jsonl"))}
    scores_path = BASE / "scores.jsonl"
    scored = set()
    if scores_path.exists():
        scored = {json.loads(l)["id"] for l in open(scores_path)}

    model = pick_model()
    print(f"judge model: {model}", flush=True)

    while True:
        pending = [p for p in sorted((BASE / "shots").glob("*.png"))
                   if p.stem not in scored]
        for p in pending:
            cid = p.stem
            try:
                s = score(model, p, meta[cid]["query"])
            except Exception as e:  # noqa: BLE001
                print(f"ERR  {cid}: {e}", flush=True)
                continue
            s = {"id": cid, "model": model, **s}
            with open(scores_path, "a") as f:
                f.write(json.dumps(s) + "\n")
            scored.add(cid)
            print(f"{s.get('overall', '?'):>2}  {cid}  {s.get('notes', '')[:60]}", flush=True)
        if (BASE / "RENDER_DONE").exists() and not pending:
            break
        time.sleep(30)

    print(f"scoring complete: {len(scored)} cards scored", flush=True)
    (BASE / "SCORE_DONE").write_text("done\n")


if __name__ == "__main__":
    main()
