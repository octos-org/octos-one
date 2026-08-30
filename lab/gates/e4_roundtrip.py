#!/usr/bin/env python3
"""E4: does a palette lifted from a mockup survive the trip into a real card?

E0 showed mockups carry a consistent quality gradient. E1 showed Opus and GLM
agree closely on what a mockup's ground and ink ARE (median deltaE 2.8 on
ground). Neither says whether any of that transfers: a palette can be extracted
perfectly and still look worse once it is painting a different layout with
different content.

So build the same card three ways — shipped palette, Opus's extraction, GLM's
extraction — and put them to the paired judge. Both judges, both orders.

If the extractions lose to the shipped palette, extraction is a step for nothing.
If they win, the mockup is transferring something real.
"""
import itertools
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import shoot  # noqa: E402

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE / "e4"
CARD = HERE / "samples" / "good"


def rgb(h):
    h = (h or "").strip().lstrip("#")
    if len(h) != 6:
        return None
    try:
        return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))
    except ValueError:
        return None


def lum(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def mix(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def _lin(c):
    c /= 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def rel_lum(c):
    return 0.2126 * _lin(c[0]) + 0.7152 * _lin(c[1]) + 0.0722 * _lin(c[2])


def contrast(a, b):
    lo, hi = sorted((rel_lum(a), rel_lum(b)))
    return (hi + 0.05) / (lo + 0.05)


def clamp_ink(g, i, target=4.5):
    """Push the extracted ink away from the ground until body text is legible.

    Extraction reads a mockup's ink correctly, but a mockup's ink was chosen for
    a mockup's ground. Carried onto a card whose ground came from somewhere else
    — or onto the same ground with a different amount of it — the pair can land
    below AA, and in the first run some landed dark-on-dark. Nothing downstream
    caught it: the cheap judge is blind to contrast, and the expensive one only
    saw it after the render was already paid for.
    """
    if contrast(g, i) >= target:
        return i
    # Pick the direction by which one actually reaches further, not by a
    # luminance threshold: against a mid-luminance ground like terracotta,
    # white tops out at 4.2:1 while black reaches 6.2:1.
    dest = max(((0, 0, 0), (255, 255, 255)), key=lambda d: contrast(g, d))
    for k in range(1, 21):
        c = mix(i, dest, k / 20)
        if contrast(g, c) >= target:
            return c
    return dest


def lift(g, i, target=1.12):
    """A panel that separates from its page, whatever the page is.

    The shipped palettes never state a panel colour — dark lays 18/255 white
    over near-black, light lays opaque white over near-white, and both land near
    1.12:1. That is a RATIO, and it self-scales. The first transfer copied an
    absolute colour instead, so on a near-black extracted ground the panel
    vanished and Opus said so three times.
    """
    for dest in ((255, 255, 255), (0, 0, 0)):
        for k in range(1, 41):
            c = mix(g, dest, k / 200)
            if contrast(g, c) >= target:
                return c
    return mix(g, i, 0.06)


# The photo palette lays a GRADIENT over the photograph rather than a fill:
# `l0_scrim_top` at 56/255 lets the picture through, `l0_scrim` at 242/255 is
# nearly opaque where the text sits. Those alphas are the mechanism, so the
# transfer carries them and only re-colours them.
SCRIM_TOP_A, SCRIM_A = 56, 242
# What a photograph is worth assuming when clamping. A real image contains
# everything, so a scrim has to hold against the region that hurts THIS ink —
# a dark photo under dark ink, a bright one under light ink. Mid-grey was the
# first guess and it is the AVERAGE, not the worst: it passed a light palette
# at 4.5:1 that measured 1.3:1 over the same card's shadows, and three judge
# verdicts duly said "grey-on-grey hero is illegible".
PHOTO_EXTREMES = ((20, 20, 20), (235, 235, 235))


def over(src_a, src_rgb, dst):
    """src at alpha src_a composited over dst."""
    a = src_a / 255.0
    return tuple(round(s * a + d * (1 - a)) for s, d in zip(src_rgb, dst))


# What the SHIPPED photo palette achieves by this same measure — near-black
# scrim at 56/255, pure white ink. Text over a photograph cannot reach WCAG's
# 4.5:1 without largely hiding the photograph, and the incumbent does not try:
# it measures 1.9:1 against the brightest region a picture can contain.
#
# So the transfer is held to PARITY, not to an absolute the surface cannot
# support. Demanding 4.5:1 pushed every extracted scrim to 136-252/255, which
# is a photo card with the photo turned off — and that trade was measured
# earlier this session and lost: raising a scrim to reach AA reached 3.37:1 and
# the paired judge preferred the older, less legible build.
def _shipped_floor():
    ship_ground, ship_ink = (10, 15, 26), (255, 255, 255)
    return min(contrast(over(56, ship_ground, p), ship_ink) for p in PHOTO_EXTREMES)


def scrim_top_alpha(g, i, target=None):
    """How opaque the TOP of the scrim has to be for this ink to survive.

    On a `Photo` root — 41% of the corpus — text never touches `l0_base`. It sits
    on a photograph, and the palette earns legibility with a gradient: 242/255
    where the text is dense, 56/255 at the top where the picture shows through.

    The bottom stop was never the risk. At 242 it is essentially the ground
    colour, so clamping against it changes almost nothing (15.3:1 -> 14.5:1
    across the extraction set). The risk is the TOP, where a photograph is
    whatever it is and a mid-tone ink disappears into it — which is what three
    judge verdicts said: "tan on mist washes out", "gold-on-teal dims labels".

    Washing the ink toward white would fix that and throw away the palette's
    identity, which is the one thing extraction is for. So hold the ink and
    solve for the scrim instead — darken the photograph until the ink reads,
    which is what a designer does. Returns the shipped alpha when the ink
    already survives, so a palette that needs nothing pays nothing.
    """
    target = target if target is not None else _shipped_floor()
    for alpha in range(SCRIM_TOP_A, 256, 4):
        if all(contrast(over(alpha, g, p), i) >= target for p in PHOTO_EXTREMES):
            return alpha
    return 255


def palette_src(ground, ink, accent):
    """Rebuild the whole colour system from a ground and an ink.

    The first version of this set only `l0_base` and `l0_text` and produced an
    unreadable screen: the shipped dark palette expresses its secondary inks and
    every overlay as WHITE at low alpha — `l0_soft` at 230, `l0_dim` at 153,
    `l0_fill` at 18, `l0_hairline` at 26 — all of which assume a dark ground.
    Drop a cream ground under them and the captions and icons vanish.

    So nothing here is copied; every value is rebuilt from the SAME RELATIONSHIP
    it had, expressed as a position on the ground-to-ink axis. That is
    polarity-agnostic: it produces white-ish overlays under a dark ground and
    dark ones under a light ground, without ever asking which it is.

    `l0_base_2`, the bottom of the page gradient, is pushed AWAY from the panel.
    A gradient that ends on the panel colour has separation zero — a defect
    measured by hand earlier this session.
    """
    g, i0, a = rgb(ground), rgb(ink), rgb(accent)
    if not (g and i0):
        return None
    i = clamp_ink(g, i0)                 # legible against the page
    top_a = scrim_top_alpha(g, i)        # and against a photograph, via the scrim
    fill = lift(g, i)                    # a panel that separates, as a ratio
    sheet = mix(g, fill, 0.7)
    base2 = mix(g, (0, 0, 0) if rel_lum(g) > 0.35 else (255, 255, 255), 0.06)
    a = clamp_ink(g, a or i, 3.0)        # an accent still has to be seen

    def toward_ink(t):
        return mix(g, i, t)

    def hx(c):
        return f"argb(255, {c[0]}, {c[1]}, {c[2]})"

    return "\n".join([
        "// extracted palette (E4 v2) — spliced into the axis slot.",
        f"// ink clamped {contrast(g, i0):.1f}:1 -> {contrast(g, i):.1f}:1"
        f" · panel lift {contrast(g, fill):.2f}:1"
        f" · scrim top {SCRIM_TOP_A} -> {top_a}"
        f" ({min(contrast(over(top_a, g, p), i) for p in PHOTO_EXTREMES):.1f}:1"
        f" over the worst photo)",
        f"let l0_base     = {hx(g)}",
        f"let l0_base_2   = {hx(base2)}",
        f"let l0_sheet    = {hx(sheet)}",
        f"let l0_fill     = {hx(fill)}",
        f"let l0_hairline = {hx(toward_ink(0.14))}",
        f"let l0_stroke   = {hx(toward_ink(0.18))}",
        f"let l0_active   = {hx(toward_ink(0.20))}",
        f"let l0_bar_rail = {hx(toward_ink(0.16))}",
        f"let l0_text     = {hx(i)}",
        f"let l0_soft     = {hx(toward_ink(0.90))}",
        f"let l0_dim      = {hx(toward_ink(0.66))}",
        # icons take l0_text when icon_mono is set, so pin it — a palette that
        # left it at the mood's value inherited whatever polarity the mood had.
        "let icon_mono   = 1",
        f"let l0_accent   = {hx(a)}",
        "let l0_bar      = l0_accent",
        "let l0_go       = l0_accent",
        # A Photo root never paints l0_base under its text. Re-colour the
        # gradient the photo palette lays over the picture, keeping its alphas:
        # those are the mechanism, and the colour is what the extraction has.
        f"let l0_scrim_top = argb({top_a}, {g[0]}, {g[1]}, {g[2]})",
        f"let l0_scrim     = argb({SCRIM_A}, {g[0]}, {g[1]}, {g[2]})",
        "",
    ])


def main():
    import synth_data
    ex = json.loads((HERE / "extract_results.json").read_text())
    labels = json.loads((HERE / "samples" / "labels.json").read_text())
    corpus = HERE.parent / "style-factory" / "corpus"
    want = int(sys.argv[1]) if len(sys.argv) > 1 else 6
    # The CARD, not the lowered DSL: a pre-lowered card has its colours baked in
    # as literals, so the kit never runs and an override changes nothing.
    #
    # And skip the two card shapes a palette cannot reach. A `Photo` root puts
    # its text on a PHOTOGRAPH, where legibility comes from the scrim and not
    # from `l0_base`; an emoji used as an icon carries its own colour-font
    # colour and ignores `draw_text.color` entirely. The first run of this
    # experiment drew six cards and every one of them hit one or the other —
    # 0/6 clean against a corpus rate of 54% — so its verdict was about the
    # blockers, not about the transfer.
    import re
    EMOJI = re.compile("[\U0001F000-\U0001FAFF\u2600-\u27bf\ufe0f]")

    # `--photo` selects the arm the transfer could not reach until the scrim
    # was carried across: a Photo root, where the text never touches l0_base.
    photo_arm = "--photo" in sys.argv

    def selects(path):
        t = path.read_text()
        if any(EMOJI.search(m.group(2))
               for m in re.finditer(r'(text|glyph)\s*:\s*"([^"]*)"', t)):
            return False          # emoji are untintable in either arm
        if re.search(r"\b(Map|NavMap)\(", t):
            return False          # a live map is a third unreachable surface
        return bool(re.search(r"^view root Photo\(", t, re.M)) == photo_arm

    cards = [(sid, corpus / labels[sid]["card"]) for sid in sorted(labels)
             if selects(corpus / labels[sid]["card"])][:want]
    OUT.mkdir(exist_ok=True)
    (OUT / "pal").mkdir(exist_ok=True)
    (OUT / "data").mkdir(exist_ok=True)

    # one mockup per card, cycling — each card gets a different design's palette
    mocks = sorted(ex)
    plan = []
    for n, (sid, card) in enumerate(cards):
        mock = mocks[n % len(mocks)]
        data = OUT / "data" / f"{sid}.json"
        if not data.exists():
            data.write_text(json.dumps(synth_data.synth(card.read_text())))
        for who in ("shipped", "opus", "glm"):
            png = OUT / f"{sid}-{who}.png"
            pal = None
            if who != "shipped":
                d = ex[mock].get(who) or {}
                src = palette_src(d.get("ground"), d.get("ink"), d.get("accent"))
                if src is None:
                    continue
                pal = OUT / "pal" / f"{sid}-{who}.splash"
                pal.write_text(src)
            plan.append((sid, card, who, mock, pal, png, data))

    done = 0
    for sid, card, who, mock, pal, png, data in plan:
        if png.exists():
            done += 1
            print(f"  {sid}-{who:<8} cached", flush=True)
            continue
        ok = shoot.shoot(card, png, pal, data=data)
        done += ok
        print(f"  {sid}-{who:<8} {card.name[:22]:<24} from {mock[:20]:<22} "
              f"{'ok' if ok else 'FAILED'}", flush=True)

    (OUT / "plan.json").write_text(json.dumps(
        [{"sid": s, "card": c.name, "arm": w, "mockup": m, "png": p.name}
         for s, c, w, m, _, p, _ in plan], indent=1))
    print(f"\n{done}/{len(plan)} rendered -> {OUT}")


if __name__ == "__main__":
    main()
