#!/usr/bin/env python3
"""Judge E4: shipped palette against each extraction, both orders, both judges.

The question is narrow on purpose. Not "is this card good" — the layout, the
content and the type are identical across the three arms, so the ONLY difference
is the colour system. A win here is attributable to the palette and nothing else.

Both judges run, because E0 showed they disagree systematically: GLM rewards
minimalism where Opus reads it as unresolved. A palette result that only one of
them likes is a result about that judge.

Usage:  e4_judge.py [--judge opus|glm|both] [--workers N]
"""
import base64
import concurrent.futures as cf
import io
import json
import os
import pathlib
import re
import subprocess
import sys
import urllib.request

from PIL import Image

HERE = pathlib.Path(__file__).resolve().parent
E4 = HERE / "e4"
OUT = HERE / "e4_judged.json"
URL = "https://api.z.ai/api/coding/paas/v4/chat/completions"

PROMPT = """Two renders of the SAME phone card. Same layout, same content, same
type — the only difference is the colour system.

Which colour system serves the card better? Judge legibility first, then whether
the palette reads as chosen rather than default, and whether emphasis lands where
it should.

Answer on ONE line, exactly this shape and nothing else:
WINNER=<A or B> | <at most 12 words saying why>"""


def key():
    k = os.environ.get("ZAI_KEY", "")
    if not k and os.environ.get("ZAI_KEY_FILE"):
        k = pathlib.Path(os.environ["ZAI_KEY_FILE"]).read_text().strip()
    return k


def enc(p, cache={}):
    if p not in cache:
        im = Image.open(p).convert("RGB")
        im.thumbnail((560, 1200))
        b = io.BytesIO()
        im.save(b, "JPEG", quality=80)
        cache[p] = base64.b64encode(b.getvalue()).decode()
    return cache[p]


def glm(a, b):
    body = {"model": "glm-5.3-flash", "max_tokens": 160, "temperature": 0.2,
            "thinking": {"type": "disabled"},
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "The first image is A, the second is B.\n\n" + PROMPT},
                {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{enc(a)}"}},
                {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{enc(b)}"}}]}]}
    req = urllib.request.Request(URL, data=json.dumps(body).encode(),
                                 headers={"Authorization": f"Bearer {key()}",
                                          "Content-Type": "application/json"})
    for _ in range(3):
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                d = json.load(r)
            m = re.search(r"WINNER\s*=\s*([AB])\s*\|?\s*(.*)",
                          d["choices"][0]["message"].get("content") or "")
            if m:
                return m.group(1), m.group(2).strip()[:70]
        except Exception:
            continue
    return None, "failed"


def opus(a, b):
    r = subprocess.run(
        ["claude", "-p", f"Read both images:\n  A: {a.resolve()}\n  B: {b.resolve()}\n\n{PROMPT}",
         "--model", "opus", "--allowedTools", "Read"],
        capture_output=True, text=True, timeout=600)
    m = re.search(r"WINNER\s*=\s*([AB])\s*\|?\s*(.*)", r.stdout)
    return (m.group(1), m.group(2).strip()[:70]) if m else (None, "unparsed")


JUDGES = {"glm": glm, "opus": opus}


def main():
    which = sys.argv[sys.argv.index("--judge") + 1] if "--judge" in sys.argv else "both"
    workers = int(sys.argv[sys.argv.index("--workers") + 1]) if "--workers" in sys.argv else 5
    plan = json.loads((E4 / "plan.json").read_text())
    sids = sorted({p["sid"] for p in plan})
    mock = {p["sid"]: p["mockup"] for p in plan}

    jobs = []
    for sid in sids:
        for arm in ("opus", "glm"):
            if not (E4 / f"{sid}-{arm}.png").exists():
                continue
            for swap in (0, 1):
                for j in (["glm", "opus"] if which == "both" else [which]):
                    jobs.append((sid, arm, swap, j))

    def run(job):
        sid, arm, swap, j = job
        s, e = E4 / f"{sid}-shipped.png", E4 / f"{sid}-{arm}.png"
        a, b = (e, s) if swap else (s, e)
        w, why = JUDGES[j](a, b)
        if w is None:
            return None
        first, second = ("extracted", "shipped") if swap else ("shipped", "extracted")
        return {"sid": sid, "arm": arm, "swap": swap, "judge": j,
                "winner": first if w == "A" else second, "why": why,
                "mockup": mock[sid]}

    res = []
    with cf.ThreadPoolExecutor(workers) as ex:
        for n, r in enumerate(ex.map(run, jobs), 1):
            if r:
                res.append(r)
                print(f"  [{n}/{len(jobs)}] {r['judge']:<5} {r['sid']} {r['arm']:<5}"
                      f" swap={r['swap']} -> {r['winner']:<10} {r['why'][:44]}", flush=True)
            else:
                print(f"  [{n}/{len(jobs)}] failed", flush=True)

    OUT.write_text(json.dumps(res, indent=1))
    print(f"\n{len(res)}/{len(jobs)} judged -> {OUT}")


if __name__ == "__main__":
    main()
