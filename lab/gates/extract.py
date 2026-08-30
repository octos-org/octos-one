#!/usr/bin/env python3
"""E1/E2/E3: lift palette, type ratios and composition from a mockup.

Three extractors, deliberately independent, run over the same eight mockups:

  cv     pure pixels — k-means ground, projection-profile bands. No model.
  opus   structured output from Opus, via the Read tool
  glm    structured output from glm-5.3-flash, images inline

Independence is the point. Agreement between a model and the CV baseline is
evidence the value is real; disagreement locates where extraction is guessing.
E1 (palette) has a strong reference — pixels do not have opinions about their own
colour. E2 (type ratios) and E3 (composition) do not, so there the three
extractors only bound each other.

The schema is deliberately RELATIVE. Absolute pixel sizes are meaningless from a
mockup, which carries no dpi and no render scale, and font names are ~80% top-5
at best and irrelevant when you ship one font. Ratios and proportions survive
the translation to L0, which is itself expressed in roles and factors.

Usage:  extract.py [--only cv|opus|glm] [--workers N]
"""
import base64
import collections
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
POOL = HERE / "e0"
OUT = HERE / "extract_results.json"
URL = "https://api.z.ai/api/coding/paas/v4/chat/completions"


def zai_key():
    k = os.environ.get("ZAI_KEY", "")
    if not k and os.environ.get("ZAI_KEY_FILE"):
        k = pathlib.Path(os.environ["ZAI_KEY_FILE"]).read_text().strip()
    return k


SCHEMA = """{
  "ground": "#rrggbb",        // the dominant background colour
  "ink": "#rrggbb",           // the colour most body text is set in
  "accent": "#rrggbb",        // the one colour used for emphasis, or repeat ground if none
  "ground_share": 0.0,        // fraction of the image the ground occupies, 0..1
  "hero_to_body": 1.0,        // height of the largest text element / a body line
  "distinct_sizes": 1,        // how many clearly different type sizes are used
  "margin_fraction": 0.0,     // side margin as a fraction of image width
  "bands": 1,                 // horizontal bands of content
  "hero_align": "left"        // left | center | right
}"""

PROMPT = ("Read this UI mockup and report its design system as JSON.\n\n"
          "Report RELATIVE values, not pixels — the image has no known scale.\n\n"
          f"Reply with ONLY this JSON object, no prose, no fences:\n{SCHEMA}")


# ------------------------------------------------------------------ cv

def _small(p, w=320):
    im = Image.open(p).convert("RGB")
    return im.resize((w, int(im.height * w / im.width)))


def cv_extract(p):
    im = _small(p)
    W, H = im.size
    q = im.quantize(colors=16).convert("RGB")
    px = list(q.getdata())
    counts = collections.Counter(px).most_common()
    ground, gn = counts[0]

    def lum(c):
        return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]

    # ink: the frequent colour furthest in luminance from the ground
    ink = max(counts[:8], key=lambda kv: abs(lum(kv[0]) - lum(ground)))[0]
    # accent: the most saturated frequent colour that is neither
    def sat(c):
        return (max(c) - min(c)) / max(1, max(c))
    cand = [kv[0] for kv in counts[:10] if kv[0] not in (ground, ink)]
    accent = max(cand, key=sat) if cand else ground

    # bands, by projection profile against the ground
    g = im.convert("L")
    gp = g.load()
    bgl = collections.Counter(g.getdata()).most_common(1)[0][0]
    rows, run, bands = [], None, []
    for y in range(H):
        busy = sum(1 for x in range(0, W, 3) if abs(gp[x, y] - bgl) > 40)
        if busy > W * 0.02 and run is None:
            run = y
        elif busy <= W * 0.02 and run is not None:
            if y - run > 3:
                bands.append((run, y))
            run = None
    if run is not None:
        bands.append((run, H))
    hs = sorted(b[1] - b[0] for b in bands) or [1]
    med = hs[len(hs) // 2]

    # left margin: the smallest x holding ink, across all bands
    left = W
    for a, b in bands:
        for x in range(W):
            if any(abs(gp[x, y] - bgl) > 40 for y in range(a, b, 2)):
                left = min(left, x)
                break

    # hero alignment: centre of mass of the tallest band
    align = "left"
    if bands:
        a, b = max(bands, key=lambda r: r[1] - r[0])
        xs = [x for x in range(W) for y in range(a, b, 3) if abs(gp[x, y] - bgl) > 40]
        if xs:
            c = (min(xs) + max(xs)) / 2 / W
            align = "center" if 0.4 < c < 0.6 else ("right" if c >= 0.6 else "left")

    hexs = lambda c: "#%02x%02x%02x" % c
    return {"ground": hexs(ground), "ink": hexs(ink), "accent": hexs(accent),
            "ground_share": round(gn / len(px), 3),
            "hero_to_body": round(hs[-1] / max(1, med), 2),
            "distinct_sizes": len({h // 4 for h in hs}),
            "margin_fraction": round(left / W, 3),
            "bands": len(bands), "hero_align": align}


# ------------------------------------------------------------------ models

def _json_from(txt):
    m = re.search(r"\{.*\}", txt, re.S)
    if not m:
        return None
    try:
        return json.loads(re.sub(r"//[^\n\"]*", "", m.group(0)))
    except Exception:
        return None


def opus_extract(p):
    r = subprocess.run(
        ["claude", "-p", f"Read the image at {p.resolve()}\n\n{PROMPT}",
         "--model", "opus", "--allowedTools", "Read"],
        capture_output=True, text=True, timeout=600)
    return _json_from(r.stdout)


def glm_extract(p, model="glm-5.3-flash"):
    im = _small(p, 640)
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=80)
    b64 = base64.b64encode(buf.getvalue()).decode()
    body = {"model": model, "max_tokens": 400, "temperature": 0.1,
            "thinking": {"type": "disabled"},
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": PROMPT},
                {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{b64}"}}]}]}
    req = urllib.request.Request(
        URL, data=json.dumps(body).encode(),
        headers={"Authorization": f"Bearer {zai_key()}", "Content-Type": "application/json"})
    for _ in range(3):
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                d = json.load(r)
            return _json_from(d["choices"][0]["message"].get("content") or "")
        except Exception:
            continue
    return None


def main():
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    workers = int(sys.argv[sys.argv.index("--workers") + 1]) if "--workers" in sys.argv else 4
    shots = sorted(p for p in POOL.glob("*.png") if "sheet" not in p.name)
    out = json.loads(OUT.read_text()) if OUT.exists() else {}

    jobs = []
    for p in shots:
        for who, fn in (("cv", cv_extract), ("opus", opus_extract), ("glm", glm_extract)):
            if only and who != only:
                continue
            jobs.append((p.stem.replace("-mockup", ""), who, p, fn))

    def run(job):
        name, who, p, fn = job
        try:
            return name, who, fn(p)
        except Exception as e:
            return name, who, {"error": str(e)[:80]}

    with cf.ThreadPoolExecutor(workers) as ex:
        for name, who, res in ex.map(run, jobs):
            out.setdefault(name, {})[who] = res
            ok = "ok" if res and "error" not in res else "FAILED"
            print(f"  {who:<5} {name[:26]:<28} {ok}", flush=True)

    OUT.write_text(json.dumps(out, indent=1))
    print(f"\n-> {OUT}")


if __name__ == "__main__":
    main()
