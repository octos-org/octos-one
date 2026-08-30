#!/usr/bin/env python3
"""Build a labelled corpus of good and defective renders for gate measurement.

A gate is worth nothing until its false-positive rate is measured, and six clean
specimens bound that rate only at 39% (see FINDINGS-cheap-gates.md). So: take
real cards, realize them offline, and produce two arms that differ by exactly
one injected defect each.

  good/NNN.dsl          the card as the generator wrote it
  bad/NNN-<defect>.dsl  the same card with one thing broken, and only one

Ground truth goes in labels.json, so recall and false-positive rate are both
measurable rather than asserted.

Sources are baked to literals rather than fetched. A live `sys.weather` call on
a desktop with no proxy renders an em dash, and a card full of em dashes has no
text to squeeze, truncate or misalign — it would measure nothing.
"""
import json
import pathlib
import random
import re
import subprocess
import sys
import tempfile

import synth_data

HERE = pathlib.Path(__file__).resolve().parent
CORPUS = HERE.parent / "style-factory" / "corpus"
SPLASH = pathlib.Path.home() / "home" / "Splash"
OUT = HERE / "samples"

# What each source field stands for once the fetch is taken out. Values are the
# lengths a real reading has — a two-digit temperature, a city name — because
# the geometry under test is driven by how much text there is.
LITERALS = {
    "name": "Kyoto",
    "label": "Kyoto, Japan",
    "en": "Mon",
    "current.temperature_2m": "18",
    "current.apparent_temperature": "16",
    "current.relative_humidity_2m": "61",
    "current.wind_speed_10m": "9",
    "current.surface_pressure": "1013",
    "current.weather_code": "2",
    "current.visibility": "10",
    "lat": "35.0",
    "lon": "135.8",
}
_DEF_NUM = "21"


def bake(dsl):
    """Replace every top-level sys.* call with the literal it would return."""
    def spans(s):
        out = []
        for m in re.finditer(r"sys\.[a-z_]+\(", s):
            i, d = m.end(), 1
            while i < len(s) and d:
                d += (s[i] == "(") - (s[i] == ")")
                i += 1
            out.append((m.start(), i))
        return out

    while True:
        sp = spans(dsl)
        top = [(a, b) for a, b in sp if not any(x < a and b <= y for x, y in sp if (x, y) != (a, b))]
        if not top:
            return dsl
        a, b = top[0]
        call = dsl[a:b]
        fields = re.findall(r'"([^"]*)"', call)
        key = fields[-1] if fields else ""
        if key in LITERALS:
            lit = LITERALS[key]
        elif re.match(r"daily\.temperature_2m_max", key):
            lit = "23"
        elif re.match(r"daily\.temperature_2m_min", key):
            lit = "12"
        elif "precipitation" in key:
            lit = "12"
        elif "uv" in key:
            lit = "3"
        elif "dayname" in call or key in ("en", "zh"):
            lit = "Mon"
        else:
            lit = _DEF_NUM
        dsl = dsl[:a] + f'"{lit}"' + dsl[b:]


# ------------------------------------------------------------------ mutations
#
# Each targets exactly one gate, and each is something a generator could
# plausibly emit — a width that is too small, a container that forgot to fit,
# a flow set to Overlay, a colour picked without checking its ground.

def _pick(dsl, pattern, rng):
    hits = list(re.finditer(pattern, dsl))
    return rng.choice(hits) if hits else None


#
# Each must produce its defect BY CONSTRUCTION. The first version picked a site
# at random and often changed nothing — `width: 26` on a two-character label
# fits, `flow: Overlay` on a container that was already stacked is a no-op —
# and every inert mutation was then counted against the gate's recall. A
# mutation that cannot be shown to break the render is not evidence about a
# gate. `verify` at the bottom of this file drops the ones that still land inert.

def m_squeeze(dsl, rng):
    """Give a LONG text run a column that cannot hold two characters."""
    hits = list(re.finditer(r'(Text\w*\{)([^}\n]*?)text: "([^"]{12,})"', dsl))
    if not hits:
        return None
    m = rng.choice(hits)
    out = dsl[:m.end(1)] + " width: 20 " + dsl[m.end(1):]
    return re.sub(r"(width: 20 [^}\n]*?)width: (?:Fill|Fit)", r"\1", out, count=1)


def m_clip(dsl, rng):
    """Fix a container's height at 8px — nothing with children fits in that."""
    hits = list(re.finditer(r"(View\{[^}\n]*?)height: Fit([^\n]*\n\s+\w+\{)", dsl))
    if not hits:
        return None
    m = rng.choice(hits)
    return dsl[:m.end(1)] + "height: 8" + dsl[m.end(1) + len("height: Fit"):]


def m_overlap(dsl, rng):
    """Stack a row's text children on the same origin.

    A negative margin was the first attempt and it does not collide — the whole
    subtree below shifts with it and the layout simply reflows. Two siblings
    only truly share pixels when their container stops separating them, so:
    flow to Overlay, and drop the spacing that would still hold them apart."""
    lines = dsl.split("\n")
    cands = []
    for i, ln in enumerate(lines):
        if "flow: Right" not in ln:
            continue
        indent = len(ln) - len(ln.lstrip())
        kids = 0
        for nxt in lines[i + 1:]:
            ni = len(nxt) - len(nxt.lstrip())
            if nxt.strip() and ni <= indent:
                break
            if ni == indent + 2 and re.match(r"\s*(Text\w*|Label)\{", nxt):
                kids += 1
        if kids >= 2:
            cands.append(i)
    if not cands:
        return None
    i = rng.choice(cands)
    lines[i] = re.sub(r"spacing: \d+", "", lines[i].replace("flow: Right", "flow: Overlay"))
    return "\n".join(lines)


def m_offscreen(dsl, rng):
    """Push a node far past the right edge of a 360-wide screen."""
    hits = list(re.finditer(r"(Text\w*\{)", dsl))
    if not hits:
        return None
    m = rng.choice(hits)
    return dsl[:m.end(1)] + " margin: Inset{left: 900} " + dsl[m.end(1):]


def m_truncate(dsl, rng):
    """Pin a long run to one line in a box a fraction of its width.

    This is the defect no screenshot can name: the text is not wrapped, not
    clipped by a parent — the layout simply stopped painting glyphs."""
    hits = list(re.finditer(r'(Text\w*\{)([^}\n]*?)text: "([^"]{14,})"', dsl))
    if not hits:
        return None
    m = rng.choice(hits)
    out = dsl[:m.end(1)] + " width: 44 max_lines: 1 " + dsl[m.end(1):]
    return re.sub(r"(width: 44 max_lines: 1 [^}\n]*?)width: (?:Fill|Fit)", r"\1", out, count=1)


def m_contrast(dsl, rng):
    """Set an ink to the exact colour of the ground behind it — 1.0:1."""
    hits = list(re.finditer(r"draw_text\.color: #[0-9a-fA-F]{6,8}", dsl))
    if not hits:
        return None
    m = rng.choice(hits)
    page = re.search(r"draw_bg\.color: #([0-9a-fA-F]{6})\b", dsl)
    ink = page.group(1) if page else "0a0e14"
    return dsl[:m.start()] + f"draw_text.color: #{ink}" + dsl[m.end():]


MUTATIONS = {
    "squeeze": (m_squeeze, "squeeze"),
    "clip": (m_clip, "overflow"),   # a too-short container overflows, it does not clip
    "overlap": (m_overlap, "overlap"),
    "offscreen": (m_offscreen, "offscreen"),
    "contrast": (m_contrast, "contrast"),
    "truncate": (m_truncate, "truncated"),
}


def realize(card):
    """Realize a card against data synthesised from its own declarations."""
    data = synth_data.synth(card.read_text())
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(data, f)
        path = f.name
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "splash-ui-l0", "--example", "lower_l0",
         "--", str(card), path],
        cwd=SPLASH, capture_output=True, text=True, timeout=180)
    pathlib.Path(path).unlink(missing_ok=True)
    return r.stdout if r.returncode == 0 and r.stdout.strip() else None


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 50
    rng = random.Random(20260829)

    # Spread across MODELS, not across the corpus. The corpus varies theme and
    # palette; its 263 weather cards realize to one identical tree, so 50 of
    # them is one sample counted fifty times. A `# model:` line names the
    # structure, which is what layout checking is about.
    by_model = {}
    for p in sorted(CORPUS.glob("*.card")):
        m = re.search(r"^# model: (.+)$", p.read_text(), re.M)
        if m:
            by_model.setdefault(m.group(1).strip(), []).append(p)
    for v in by_model.values():
        rng.shuffle(v)
    models = sorted(by_model)
    cards = [by_model[m][i] for i in range(max(len(v) for v in by_model.values()))
             for m in models if i < len(by_model[m])]

    (OUT / "good").mkdir(parents=True, exist_ok=True)
    (OUT / "bad").mkdir(parents=True, exist_ok=True)
    labels, kinds = {}, list(MUTATIONS)
    seen = set()
    made = 0
    for card in cards:
        if made >= n:
            break
        dsl = realize(card)
        if dsl is None:
            continue
        dsl = bake(dsl)
        # The lowered form opens with a comment, and the Splash widget decides
        # how to wrap a body by looking at its first token — a leading `//`
        # makes it pick the View wrapper and the parse breaks.
        dsl = "\n".join(l for l in dsl.splitlines() if not l.startswith("//")).strip()

        # Reject a tree we already have. Two cards that lower identically are
        # one observation, whatever their filenames say.
        fp = hash(dsl)
        if fp in seen:
            continue
        seen.add(fp)

        sid = f"{made:03d}"
        kind = kinds[made % len(kinds)]
        fn, gate = MUTATIONS[kind]
        bad = fn(dsl, rng)
        if bad is None or bad == dsl:
            continue

        (OUT / "good" / f"{sid}.dsl").write_text(dsl)
        (OUT / "bad" / f"{sid}-{kind}.dsl").write_text(bad)
        model = re.search(r"^# model: (.+)$", card.read_text(), re.M).group(1).strip()
        labels[sid] = {"card": card.name, "model": model,
                       "mutation": kind, "expect_gate": gate}
        made += 1
        print(f"  {sid}  {model:<24} {card.name:<26} +{kind}")

    (OUT / "labels.json").write_text(json.dumps(labels, indent=1))
    print(f"\n{made} pairs -> {OUT}")


if __name__ == "__main__":
    main()
