#!/usr/bin/env python3
"""E0 again, with glm-5.3-flash instead of Opus — is the cheap judge good enough?

Opus produced a clean gradient on these eight mockups: 86% self-agreement under
order swap, 36/36 transitive triads, a full ladder from 7/7 to 0/7. That is the
reference. This runs the IDENTICAL prompt and the identical 56 comparisons
through a much cheaper model, so the two are directly comparable.

Three things have to hold before the cheap judge can carry the volume work:

  self-agreement   it must read the images, not the presentation order
  transitivity     the wins must collapse to one ordering, not a tangle
  concordance      that ordering must match Opus's, or it is measuring
                   something else cheaply

Usage:  e0_judge_zai.py [--workers N] [--model glm-5.3-flash]
"""
import base64
import concurrent.futures as cf
import io
import itertools
import json
import os
import pathlib
import re
import sys
import urllib.error
import urllib.request

from PIL import Image

HERE = pathlib.Path(__file__).resolve().parent
POOL = HERE / "e0"
OUT = HERE / "e0_results_zai.json"
# Key from ZAI_KEY, or a file named by ZAI_KEY_FILE. Never checked in.
_k = os.environ.get("ZAI_KEY", "")
if not _k and os.environ.get("ZAI_KEY_FILE"):
    _k = pathlib.Path(os.environ["ZAI_KEY_FILE"]).read_text().strip()
if not _k:
    sys.exit("set ZAI_KEY or ZAI_KEY_FILE")
KEY = _k
URL = "https://api.z.ai/api/coding/paas/v4/chat/completions"

# Word for word the prompt Opus was given, minus the file paths — the images
# arrive inline here. Changing it would make the two runs incomparable.
PROMPT = """You are judging two UI design mockups for a phone card. The first
image is A, the second is B.

Which is the better DESIGN? Judge composition, typographic hierarchy, restraint,
and how confidently the eye is led — not which subject you find more interesting,
and not the content. Ignore that they show different data.

Answer on ONE line, exactly this shape and nothing else:
WINNER=<A or B> | <at most 12 words saying why>"""


def encode(p, cache={}):
    if p not in cache:
        im = Image.open(p).convert("RGB")
        im.thumbnail((640, 1400))
        buf = io.BytesIO()
        im.save(buf, "JPEG", quality=80)
        cache[p] = base64.b64encode(buf.getvalue()).decode()
    return cache[p]


def judge(a, b, model):
    body = {
        "model": model,
        "max_tokens": 160,
        "temperature": 0.2,
        "thinking": {"type": "disabled"},
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": PROMPT},
            {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{encode(a)}"}},
            {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{encode(b)}"}},
        ]}],
    }
    req = urllib.request.Request(
        URL, data=json.dumps(body).encode(),
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
    for _ in range(3):
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                d = json.load(r)
            txt = d["choices"][0]["message"].get("content") or ""
            m = re.search(r"WINNER\s*=\s*([AB])\s*\|?\s*(.*)", txt)
            if m:
                return m.group(1), m.group(2).strip()[:70], d.get("usage", {})
            return None, txt[:90], d.get("usage", {})
        except urllib.error.HTTPError as e:
            last = e.read().decode()[:90]
        except Exception as e:
            last = str(e)[:90]
    return None, last, {}


def main():
    workers = 6
    model = "glm-5.3-flash"
    if "--workers" in sys.argv:
        workers = int(sys.argv[sys.argv.index("--workers") + 1])
    if "--model" in sys.argv:
        model = sys.argv[sys.argv.index("--model") + 1]

    shots = sorted(p for p in POOL.glob("*.png") if "sheet" not in p.name)
    names = [p.stem.replace("-mockup", "") for p in shots]
    jobs = [(i, j, s) for i, j in itertools.combinations(range(len(shots)), 2) for s in (0, 1)]
    print(f"{model}: {len(shots)} mockups, {len(jobs)} comparisons\n")

    tok = [0, 0]

    def run(job):
        i, j, swap = job
        a, b = (shots[j], shots[i]) if swap else (shots[i], shots[j])
        w, why, usage = judge(a, b, model)
        tok[0] += usage.get("prompt_tokens", 0)
        tok[1] += usage.get("completion_tokens", 0)
        if w is None:
            return {"i": i, "j": j, "swap": swap, "winner": None, "why": why}
        first, second = (j, i) if swap else (i, j)
        return {"i": i, "j": j, "swap": swap,
                "winner": first if w == "A" else second, "why": why}

    results = []
    with cf.ThreadPoolExecutor(workers) as ex:
        for n, r in enumerate(ex.map(run, jobs), 1):
            results.append(r)
            if r["winner"] is None:
                print(f"  [{n}/{len(jobs)}] unparsed: {r['why'][:60]}", flush=True)
            else:
                print(f"  [{n}/{len(jobs)}] {names[r['i']][:20]:<22} vs {names[r['j']][:20]:<22}"
                      f" -> {names[r['winner']][:20]:<22} {r['why'][:34]}", flush=True)

    OUT.write_text(json.dumps(
        {"names": names, "model": model,
         "results": [r for r in results if r["winner"] is not None]}, indent=1))
    ok = sum(1 for r in results if r["winner"] is not None)
    print(f"\n{ok}/{len(jobs)} judged  |  {tok[0]} prompt + {tok[1]} completion tokens  -> {OUT}")


if __name__ == "__main__":
    main()
