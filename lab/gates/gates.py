#!/usr/bin/env python3
"""Deterministic layout gates over renderer geometry. No model, no pixels.

Every gate in here answers a question the renderer already knows the answer to.
The pixel-based attempts that preceded this are recorded in
`lab/style-factory/FINDINGS-cheap-gates.md`: they failed because a screenshot
carries appearance, not intent, and reconstructing element boundaries from a
raster is guesswork. The renderer hands us the boundaries for free.

Input is one JSON document per render:

    {"screen": {"w": 1080, "h": 2340},
     "widgets": [
       {"i": 0, "parent": -1, "id": "l0n0", "kind": "View",
        "x": 0, "y": 0, "w": 1080, "h": 2340,       # what the widget asked for
        "cx": 0, "cy": 0, "cw": 1080, "ch": 2340,   # what survived clipping
        "visible": true, "text": null,
        "bg": "#ff101418", "fg": null,              # argb, null when unknown
        "tappable": false, "scroller": false, "overlay": false}]}

Three-valued on purpose. A gate that cannot see enough to decide returns UNKNOWN
rather than guessing; UNKNOWN routes onward to the judge, it does not fail the
card. Guessing is what produced a 1.0:1 contrast reading on a good render.
"""
import json
import sys
from dataclasses import dataclass

PASS, FAIL, UNKNOWN = "PASS", "FAIL", "UNKNOWN"

# Android's minimum touch target, in the density-independent pixels the L0
# renderer lays out in. Screens are captured at 3x on the test device.
MIN_TAP_DP = 48
SLOP = 6          # px of overrun treated as rounding, not defect
# Glyphs painted, as a fraction of non-space characters. Below this a run
# stopped early; above it, ligatures and emoji explain the shortfall.
GLYPH_FLOOR = 0.6
DP = float(__import__("os").environ.get("GATE_DP", "1.0"))

# WCAG 2.1: 4.5:1 for body text, 3:1 for text at 18.66dp bold / 24dp regular.
LARGE_TEXT_PX = 24 * DP


@dataclass
class Finding:
    gate: str
    verdict: str
    node: str
    detail: str

    def __str__(self):
        return f"{self.verdict:7} {self.gate:<10} {self.node:<16} {self.detail}"


# ---------------------------------------------------------------- helpers

def _rect(n):
    return n["x"], n["y"], n["x"] + n["w"], n["y"] + n["h"]


def _clip(n):
    return n["cx"], n["cy"], n["cx"] + n["cw"], n["cy"] + n["ch"]


def _inter(a, b):
    x0, y0 = max(a[0], b[0]), max(a[1], b[1])
    x1, y1 = min(a[2], b[2]), min(a[3], b[3])
    return max(0, x1 - x0) * max(0, y1 - y0)


def _area(r):
    return max(0, r[2] - r[0]) * max(0, r[3] - r[1])


def _contains(outer, inner, slack=1):
    return (outer[0] - slack <= inner[0] and outer[1] - slack <= inner[1]
            and outer[2] + slack >= inner[2] and outer[3] + slack >= inner[3])


def _drawn(n):
    """A node that puts something on screen and can therefore be wrong."""
    return n.get("visible", True) and n["w"] > 0 and n["h"] > 0


def _paints_nothing(n):
    """No text and a fully transparent fill — a grouping box, not content."""
    if n.get("text"):
        return False
    bg = _argb(n.get("bg"))
    return bg is not None and bg[0] == 0


def _lin(c):
    c /= 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def _argb(s):
    if not s:
        return None
    s = s.lstrip("#")
    if len(s) == 8:
        a, r, g, b = int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16), int(s[6:8], 16)
    elif len(s) == 6:
        a, r, g, b = 255, int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16)
    else:
        return None
    return a, r, g, b


def _lum(rgb):
    return 0.2126 * _lin(rgb[0]) + 0.7152 * _lin(rgb[1]) + 0.0722 * _lin(rgb[2])


def contrast_ratio(fg, bg):
    lo, hi = sorted((_lum(fg), _lum(bg)))
    return (hi + 0.05) / (lo + 0.05)


# ---------------------------------------------------------------- gates

def gate_offscreen(doc):
    """A node whose box leaves the screen. Arithmetic, no judgment."""
    W, H = doc["screen"]["w"], doc["screen"]["h"]
    out = []
    for n in doc["widgets"]:
        if not _drawn(n):
            continue
        x0, y0, x1, y1 = _rect(n)
        over = []
        if x0 < -1: over.append(f"left by {-x0}")
        if y0 < -1: over.append(f"top by {-y0}")
        if x1 > W + 1: over.append(f"right by {x1 - W}")
        if y1 > H + 1: over.append(f"bottom by {y1 - H}")
        # A scroller's content legitimately extends past the fold.
        # a scroller's content legitimately runs past the fold, never sideways
        if over and _in_scroller(doc, n):
            over = [o for o in over if not o.startswith(("top", "bottom"))]
        if over:
            out.append(Finding("offscreen", FAIL, f"#{n['i']}:{n['id']}",
                               f"{n['kind']} {n['w']}x{n['h']} runs off " + ", ".join(over)))
    return out or [Finding("offscreen", PASS, "-", "every box inside the screen")]


def gate_sliver(doc):
    """A visible node collapsed to nothing. Almost always a layout mistake."""
    out = []
    for n in doc["widgets"]:
        if not n.get("visible", True):
            continue
        if n.get("text") and (n["w"] < 2 or n["h"] < 2):
            out.append(Finding("sliver", FAIL, f"#{n['i']}:{n['id']}",
                               f"{n['kind']} carrying text is {n['w']}x{n['h']}"))
    return out or [Finding("sliver", PASS, "-", "no collapsed nodes")]


def _in_scroller(doc, n):
    by_i = {w["i"]: w for w in doc["widgets"]}
    p = n.get("parent", -1)
    depth = 0
    while p is not None and p >= 0 and depth < 64:
        anc = by_i.get(p)
        if anc is None:
            return False
        if anc.get("scroller"):
            return True
        p = anc.get("parent", -1)
        depth += 1
    return False


def gate_clipped(doc):
    """The widget asked for a box and got less of it. Scrollers exempt."""
    out = []
    for n in doc["widgets"]:
        if not _drawn(n) or "cw" not in n:
            continue
        r, c = _rect(n), _clip(n)
        if _area(r) == 0:
            continue
        kept = _inter(r, c) / _area(r)
        if kept < 0.995 and not _in_scroller(doc, n) and not n.get("clips_ok"):
            lost = int((1 - kept) * 100)
            out.append(Finding("clipped", FAIL, f"#{n['i']}:{n['id']}",
                               f"{n['kind']} loses {lost}% of {n['w']}x{n['h']} to a clip"))
    return out or [Finding("clipped", PASS, "-", "nothing cut by a clip")]


def gate_overlap(doc):
    """Two siblings sharing pixels. Exact to compute, and wrong to gate on
    without exclusions: scrims, badges, bars and containment all overlap on
    purpose. Only unrelated siblings in the same parent are judged."""
    by_parent = {}
    for n in doc["widgets"]:
        if _drawn(n) and not n.get("overlay"):
            by_parent.setdefault(n.get("parent", -1), []).append(n)
    out = []
    for _, kids in by_parent.items():
        for i in range(len(kids)):
            for j in range(i + 1, len(kids)):
                a, b = kids[i], kids[j]
                # L0 has exactly two intentional stacking patterns, and both
                # are identifiable: a transparent Button laid over the content
                # it makes tappable, and a scrim laid over a backdrop
                # photograph. Everything else that shares a box is a collision.
                if a.get("tappable") or b.get("tappable"):
                    continue
                if a["kind"] == "Image" or b["kind"] == "Image":
                    continue
                # A node that paints nothing cannot collide with anything. L0
                # stacks a transparent grouping View over its backdrop — photo,
                # map, gradient — and that pair is the pattern, not a defect.
                if _paints_nothing(a) or _paints_nothing(b):
                    continue
                ra, rb = _rect(a), _rect(b)
                # Containment is legitimate for a badge inside a card. Two
                # siblings sharing the SAME box are not contained, they are
                # stacked — and that is the most complete collision there is,
                # so the exemption has to be strict about size.
                small = min(_area(ra), _area(rb))
                big = max(_area(ra), _area(rb))
                if (_contains(ra, rb) or _contains(rb, ra)) and small < big * 0.8:
                    continue
                inter = _inter(ra, rb)
                if inter == 0:
                    continue
                frac = inter / min(_area(ra), _area(rb))
                if frac > 0.02:
                    out.append(Finding("overlap", FAIL, f"#{a['i']}~#{b['i']}",
                                       f"{a['kind']} and {b['kind']} share {frac*100:.0f}%"))
    return out or [Finding("overlap", PASS, "-", "no sibling collisions")]


def gate_squeeze(doc):
    """A run of text stacked vertically because its column is too narrow.

    In a left-to-right script a run is wider than it is tall. The pixel version
    of this rule fired on any tall ink block and was ~85% false, because a lone
    numeral and a weather icon are both taller than wide and a raster cannot
    tell them from broken text. The renderer hands over the STRING, which
    settles it: one glyph cannot be stacked, and an icon carries no text at
    all."""
    # Only LEAVES carry a text run. A container's `text()` returns the script
    # body it was handed — 13k characters of DSL — which is not on screen.
    parents = {w.get("parent", -1) for w in doc["widgets"]}
    out = []
    for n in doc["widgets"]:
        if not _drawn(n) or n["i"] in parents:
            continue
        text = (n.get("text") or "").strip()
        if len(text) < 2:                       # one glyph is not a wrap
            continue
        if n["h"] > n["w"] * 1.3:
            out.append(Finding("squeeze", FAIL, f"#{n['i']}:{n['id']}",
                               f"{len(text)} chars stacked into {n['w']}x{n['h']} ({text[:24]!r})"))
    return out or [Finding("squeeze", PASS, "-", "no run stacked into its column")]


def gate_overflow(doc):
    """A child that does not fit inside the parent that sizes it.

    A container given a height smaller than its content does not necessarily
    clip — makepad lets the children paint outside it — so the visible symptom
    is overflow, not a clip rect. Vertical overflow inside a scroller is how
    scrolling works and is exempt; horizontal overflow never is."""
    by_i = {w["i"]: w for w in doc["widgets"]}
    out = []
    for n in doc["widgets"]:
        if not _drawn(n):
            continue
        p = by_i.get(n.get("parent", -1))
        if p is None or not _drawn(p) or p.get("scroller"):
            continue
        r, pr = _rect(n), _rect(p)
        dx = max(pr[0] - r[0], r[2] - pr[2])
        dy = max(pr[1] - r[1], r[3] - pr[3])
        # Rects are rounded to integers at two levels, and a container's own
        # padding can leave a child flush against its edge; a few pixels of
        # overrun is measurement noise. Real overruns in the corpus are 30-80px.
        if dx > SLOP:
            out.append(Finding("overflow", FAIL, f"#{n['i']}:{n['id']}",
                               f"{n['kind']} sticks {dx}px out of {p['kind']} sideways"))
        elif dy > SLOP:
            # Only the SCROLLER itself may hold more than it shows. An inner
            # container that does not fit its children is a sizing mistake
            # wherever it sits — testing the whole ancestry instead of the
            # immediate parent exempted every container inside the chat list,
            # which is most of the card.
            out.append(Finding("overflow", FAIL, f"#{n['i']}:{n['id']}",
                               f"{n['kind']} {n['h']}px tall overruns {p['kind']} by {dy}px"))
    return out or [Finding("overflow", PASS, "-", "every child fits its parent")]


def gate_truncated(doc):
    """Text the layout gave up on: fewer glyphs painted than characters asked for.

    Asking the framework does not work here. Makepad's own `is_truncated` is
    hardcoded false for a default Label — the layouter computes it only when
    you opt into `max_lines` or an ellipsis (layouter.rs:301) — and the one
    path that does compute it drops the value unread (draw_text.rs:2164).

    But the draw call emits one quad per glyph, and that count survives in the
    retained instance buffer. A run that painted 11 quads for a 28-character
    string stopped early. This is the only post-layout evidence that a `3` on
    screen was meant to be `3.9`.

    Spaces paint nothing, so the comparison is against non-space characters.
    Ligatures and emoji sequences collapse several characters into one glyph
    and undercount: measured across 1109 real runs the benign floor was 0.75
    (`To office`, the ffi ligature) with a median of 1.00, while real
    truncation measured 0.30-0.45. The threshold sits in that gap.
    """
    saw, out = False, []
    for n in doc["widgets"]:
        if not _drawn(n) or not n.get("text") or n.get("glyphs") is None:
            continue
        saw = True
        chars = len([c for c in n["text"] if not c.isspace()])
        if chars < 4:                  # too short to tell a ligature from a loss
            continue
        if n["glyphs"] < chars * GLYPH_FLOOR:
            out.append(Finding("truncated", FAIL, f"#{n['i']}:{n['id']}",
                               f"painted {n['glyphs']} glyphs for {chars} characters "
                               f"— {n['text'][:30]!r}"))
    if not saw:
        return [Finding("truncated", UNKNOWN, "-", "no glyph counts in this dump")]
    return out or [Finding("truncated", PASS, "-", "every run painted its whole string")]


def _over(src, dst):
    """src composited over dst, both argb. Standard source-over."""
    sa = src[0] / 255.0
    return (255,) + tuple(round(src[i] * sa + dst[i] * (1 - sa)) for i in (1, 2, 3))


def _ground(doc, by_i, n):
    """The colour actually painted behind this node, or None if unknowable.

    Walking to the nearest opaque fill is not enough: L0 lays a semi-opaque
    scrim over a page fill, so the ground under a hero is the two composited.
    Anything over a photograph has no single ground and returns None — that
    abstention is the point. Guessing here is what produced 1.0:1 on a render
    that was fine."""
    chain, p, depth = [], n.get("parent", -1), 0
    while p is not None and p >= 0 and depth < 64:
        anc = by_i.get(p)
        if anc is None:
            return None
        if anc.get("kind") == "Image" or anc.get("image") or anc.get("gradient"):
            return None
        c = _argb(anc.get("bg"))
        if c and c[0] > 0:
            chain.append(c)
            if c[0] >= 250:
                break
        p, depth = anc.get("parent", -1), depth + 1
    if not chain or chain[-1][0] < 250:
        return None                        # never reached an opaque ground
    ground = chain[-1]
    for c in reversed(chain[:-1]):         # nearest layer painted last
        ground = _over(c, ground)
    return ground


def gate_contrast(doc):
    """Ink against the surface actually painted behind it.

    Over a photograph there is no single background colour, so those abstain
    rather than guess."""
    by_i = {w["i"]: w for w in doc["widgets"]}
    out, abstained, judged = [], 0, 0
    for n in doc["widgets"]:
        if not _drawn(n) or not n.get("text"):
            continue
        fg = _argb(n.get("fg"))
        bg = _ground(doc, by_i, n) if fg else None
        if fg is None or bg is None:
            abstained += 1
            continue
        ink = fg if fg[0] >= 250 else _over(fg, bg)
        judged += 1
        r = contrast_ratio(ink[1:], bg[1:])
        need = 3.0 if n["h"] >= 24 else 4.5
        if r < need:
            out.append(Finding("contrast", FAIL, f"#{n['i']}:{n['id']}",
                               f"{r:.1f}:1 needs {need}:1 — {n.get('fg')} on "
                               f"{'#%02x%02x%02x' % bg[1:]} ({(n.get('text') or '')[:18]!r})"))
    if out:
        return out
    if judged:
        return [Finding("contrast", PASS, "-",
                        f"{judged} text nodes meet WCAG AA ({abstained} over photo, abstained)")]
    return [Finding("contrast", UNKNOWN, "-",
                    f"{abstained} text nodes, none over a knowable ground")]


def gate_tap_target(doc):
    """A control smaller than a fingertip. Only meaningful on the hit box."""
    saw, out = False, []
    for n in doc["widgets"]:
        if not _drawn(n) or not n.get("tappable"):
            continue
        saw = True
        wdp, hdp = n["w"] / DP, n["h"] / DP
        if wdp < MIN_TAP_DP or hdp < MIN_TAP_DP:
            out.append(Finding("tap_target", FAIL, f"#{n['i']}:{n['id']}",
                               f"{wdp:.0f}x{hdp:.0f}dp is under {MIN_TAP_DP}dp"))
    if not saw:
        return [Finding("tap_target", UNKNOWN, "-", "no tappable nodes reported")]
    return out or [Finding("tap_target", PASS, "-", "every control at least 48dp")]


GATES = [gate_offscreen, gate_sliver, gate_clipped, gate_overflow, gate_overlap,
         gate_squeeze, gate_truncated, gate_contrast, gate_tap_target]


def run(doc):
    findings = []
    for g in GATES:
        findings.extend(g(doc))
    return findings


def verdict(findings):
    """Lexicographic: a hard defect rejects outright, it is never averaged with
    anything. This is the rule that a single 2/10 aesthetic score broke."""
    if any(f.verdict == FAIL for f in findings):
        return FAIL
    if any(f.verdict == UNKNOWN for f in findings):
        return UNKNOWN
    return PASS


if __name__ == "__main__":
    for path in sys.argv[1:]:
        doc = json.load(open(path))
        fs = run(doc)
        print(f"\n{path.rsplit('/', 1)[-1]}  ->  {verdict(fs)}")
        for f in fs:
            if f.verdict != PASS:
                print(f"   {f}")
