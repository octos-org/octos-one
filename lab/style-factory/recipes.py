#!/usr/bin/env python3
"""Sample the style space from design theory, not from vibes.

Round 1 named ten genres by feel and randomized every axis independently. That
produced incoherent cells (editorial-serif x condensed-bold x google-blue) and,
worse, zero compositional variance: all four layout values were hero-plus-list,
so the image model drew one composition 120 times (PROBE-composition.md).

This version follows TAXONOMY-codex.md's resolution order:

    1. choose composition_school + surface_model + optional digital_dialect
    2. apply their HARD EXCLUSIONS
    3. sample only the primitive values still legal
    4. record required capabilities, and which of them the DSL cannot express
    5. compile one prompt

Step 4 is the deviation from the review's advice. Codex proposed refusing to
generate a mockup whose capabilities the DSL cannot compile; that would delete
exactly the measurement this corpus exists to take. Instead every recipe carries
`dsl_gap`, so a low card score can be attributed to a KNOWN missing capability
rather than to bad generation — the priced roadmap falls out of the ledger.

Deterministic: same seed, same recipes, resumable by id.
"""
import json
import random
from pathlib import Path

# ── domains ──────────────────────────────────────────────────────────────────
# `reading` is gone: sys.reading answers the device's own saved list, which is
# empty on the handset, so round 1 measured 22 blank cards. sys.quakes is the
# USGS M2.5+ feed — world-backed, feed-shaped, and needs no device state.
DOMAINS = {
    "weather": "a weather screen: place name, current temperature as the hero, condition, and a 7-day forecast list with a low and a high per day",
    "news":    "a news reader: a section label, one lead headline with points/author/comments, and five ranked story rows with title and points/author",
    "stock":   "a market screen: one lead ticker with price and signed percent change, and five mover rows with ticker, name, price and signed change",
    "quake":   "a seismic feed: a section label, the strongest recent quake as the hero with its magnitude, and five rows with magnitude, place and how long ago",
}

# ── primitive vocabularies (TAXONOMY-codex.md §4) ────────────────────────────
TYPE = ["geometric_sans", "humanist_sans", "oldstyle_serif", "didone",
        "slab", "mono", "pixel"]
RATIO = ["1.20", "1.25", "1.333", "1.50"]
HIERARCHY = ["quiet", "clear", "poster"]
GEOMETRY = ["rectilinear", "circular_geometric", "soft_round", "pill_control",
            "mixed_geometric", "chamfered", "organic"]
CONTRAST = ["value", "scale", "weight", "hue", "spatial_isolation", "edge", "direction"]
FIGURE_GROUND = ["open", "contained", "tonal_strata", "layered", "ambiguous"]
ORNAMENT = ["none", "rules", "geometric_primitives", "organic_paths",
            "pattern", "rough_texture", "glow_scanline"]
HARMONY = ["monochrome", "analogous", "complementary", "split_complementary",
           "triadic", "achromatic_plus_one"]
KEY = ["near_black", "dark", "paper", "light", "vivid_ground"]
DENSITY = ["compact", "regular", "airy"]

# COMPOSITION is the axis round 1 lacked entirely. These are described to the
# image model as layout instructions; whether the DSL can follow them is exactly
# what the corpus measures.
COMPOSITION = {
    "hero_stack":     "a hero block at the top, the list filling the space below",
    "asymmetric":     "an asymmetric off-axis grid: the hero pushed to one side, the list indented on a different column edge",
    "void_field":     "an open field: the hero small and isolated in a large empty area, the list compressed to the lower third",
    "magazine":       "a magazine spread: an oversized lead occupying the top half, compact rows beneath in a narrower measure",
    "modular_grid":   "a modular grid of equal cells with no hero at all",
    "layered":        "layered depth: elements overlapping, the list rising over the hero's lower edge",
    "bottom_anchor":  "the hero floating high with the list anchored as a slab against the bottom edge",
}

# ── capabilities, and what the L0 profile can express TODAY ──────────────────
# Anything listed here that is not in DSL_CAN becomes `dsl_gap` on the recipe.
DSL_CAN = {"flat_fill", "radius", "hairline_rule", "weight_ramp", "size_ramp",
           "photo_background", "vertical_scrim", "mono_icon", "panel_inset",
           "single_hue_bar"}

# ── schools: hard constraints and exclusions ─────────────────────────────────
SCHOOLS = {
    "swiss": dict(
        rules="Swiss/International Typographic: asymmetric grid, strict regular rhythm, neo-grotesk type, flush-left ragged-right, generous margins, a restrained palette, no ornament beyond hairline rules",
        type=["geometric_sans", "humanist_sans"], ratio=["1.20", "1.25", "1.333"],
        geometry=["rectilinear"], ornament=["none", "rules"],
        figure_ground=["open", "contained"], composition=["asymmetric", "modular_grid"],
        surfaces=["open_flat"], caps={"hairline_rule", "flat_fill", "weight_ramp"}),
    "bauhaus": dict(
        rules="Bauhaus: geometric composition, primary triad or achromatic plus one primary, flat or outlined surfaces, circles and squares as structure, geometric sans",
        type=["geometric_sans"], ratio=["1.25", "1.333"],
        geometry=["circular_geometric", "rectilinear"], ornament=["geometric_primitives", "rules"],
        figure_ground=["open", "contained"], composition=["modular_grid", "asymmetric"],
        surfaces=["open_flat"], caps={"flat_fill", "stroke", "geometric_shape"}),
    "constructivist": dict(
        rules="Constructivist: diagonal and layered composition, dense space, bold condensed type, red-black-cream, photomontage energy",
        type=["geometric_sans", "slab"], ratio=["1.333", "1.50"],
        geometry=["rectilinear", "chamfered"], ornament=["geometric_primitives", "rules"],
        figure_ground=["layered", "ambiguous"], composition=["layered", "asymmetric"],
        surfaces=["open_flat", "hard_offset"], caps={"rotation", "stroke", "flat_fill"}),
    "de_stijl": dict(
        rules="De Stijl: modular orthogonal grid, square geometry, flat surfaces, primary triad on white, thick black rules, no curve or gradient anywhere",
        type=["geometric_sans"], ratio=["1.25", "1.333"],
        geometry=["rectilinear"], ornament=["rules", "geometric_primitives"],
        figure_ground=["contained"], composition=["modular_grid"],
        surfaces=["open_flat"], caps={"stroke", "flat_fill"}),
    "art_deco": dict(
        rules="Art Deco: axial or stepped composition, Didone or geometric display type, tracked capitals, hairline and double-frame language, metallic or jewel restraint",
        type=["didone", "geometric_sans"], ratio=["1.333", "1.50"],
        geometry=["rectilinear", "chamfered"], ornament=["rules", "geometric_primitives"],
        figure_ground=["contained", "open"], composition=["hero_stack", "magazine"],
        surfaces=["open_flat"], caps={"serif_display", "tracking", "stroke"}),
    "japanese_ma": dict(
        rules="Japanese MA: an open field where emptiness is the subject, void ratio above 0.55, muted achromatic or single-hue palette, small quiet type, one focal element",
        type=["humanist_sans", "oldstyle_serif"], ratio=["1.20", "1.25"],
        geometry=["rectilinear", "soft_round"], ornament=["none", "rules"],
        figure_ground=["open"], composition=["void_field"],
        surfaces=["open_flat"], caps={"flat_fill", "hairline_rule"}),
    "memphis": dict(
        rules="Memphis: vivid triadic or split palette, mixed geometry, syncopated rhythm, playful primitives scattered as ornament, flat outlined or hard-offset surfaces",
        type=["geometric_sans", "slab"], ratio=["1.333", "1.50"],
        geometry=["mixed_geometric", "circular_geometric"], ornament=["geometric_primitives", "pattern"],
        figure_ground=["layered", "contained"], composition=["asymmetric", "modular_grid"],
        surfaces=["open_flat", "hard_offset"], caps={"pattern", "stroke", "hard_shadow"}),
    "editorial": dict(
        rules="Editorial magazine: asymmetric grid or narrative stack, a serif display paired with a small sans, generous leading, hairline dividers, no cards or boxes",
        type=["oldstyle_serif", "didone"], ratio=["1.333", "1.50"],
        geometry=["rectilinear"], ornament=["rules", "none"],
        figure_ground=["open"], composition=["magazine", "asymmetric"],
        surfaces=["open_flat"], caps={"serif_display", "font_pair", "hairline_rule"}),
    "punk_zine": dict(
        rules="Punk zine: layered or diagonal composition, acid spot colour, extreme scale contrast, photocopied rough texture, torn edges",
        type=["slab", "mono", "geometric_sans"], ratio=["1.50"],
        geometry=["rectilinear", "chamfered"], ornament=["rough_texture", "pattern"],
        figure_ground=["layered", "ambiguous"], composition=["layered", "asymmetric"],
        surfaces=["open_flat", "hard_offset"], caps={"texture", "rotation", "stroke"}),
    "organic": dict(
        rules="Organic: flowing open composition, humanist or old-style type, analogous natural palette, soft organic masks and curves",
        type=["humanist_sans", "oldstyle_serif"], ratio=["1.25", "1.333"],
        geometry=["organic", "soft_round"], ornament=["organic_paths", "none"],
        figure_ground=["open", "layered"], composition=["void_field", "layered"],
        surfaces=["open_flat", "tonal_material"], caps={"mask", "gradient", "flat_fill"}),
}

SURFACES = {
    "open_flat":      dict(rules="flat surfaces, no card shadow, no bevel, no blur", caps=set()),
    "tonal_material": dict(rules="tonal material: cards on a page with soft elevation shadows and consistent corner radii", caps={"soft_shadow"}),
    "hard_offset":    dict(rules="neubrutalist hard offset: substantial black strokes and zero-blur offset shadows", caps={"stroke", "hard_shadow"}),
    "glass":          dict(rules="glass: translucent surfaces with backdrop blur over a non-uniform ground, visible overlap", caps={"alpha", "backdrop_blur"}),
    "relief":         dict(rules="neumorphic relief: page and surfaces share one hue, paired light and dark shadows, rounded geometry", caps={"soft_shadow", "inner_shadow"}),
}

DIALECTS = {
    None:          dict(rules="", caps=set(), forbid_schools=set()),
    "cyberpunk":   dict(rules="cyberpunk dialect: dark key, dense angular layering, emissive accent glow",
                        caps={"glow"}, forbid_schools={"japanese_ma", "organic"}),
    "retro_pixel": dict(rules="retro-pixel dialect: square geometry, a limited palette, pixel type, integer-grid icons, no blur or smooth gradient",
                        caps={"pixel_font"}, forbid_schools={"organic", "art_deco", "editorial"}),
    "vaporwave":   dict(rules="vaporwave dialect: a gradient ground with a temporal motif — grid horizon, scanline or glow",
                        caps={"gradient", "glow"}, forbid_schools={"swiss", "de_stijl", "japanese_ma"}),
    "y2k":         dict(rules="Y2K dialect: chrome, lens depth and bevelled gloss",
                        caps={"gradient", "bevel"}, forbid_schools={"swiss", "japanese_ma", "punk_zine"}),
}

# Media: photography must stop being the generator's shortcut to richness.
MEDIA = [("none", 0.40), ("abstract_geometry", 0.30), ("illustration", 0.15),
         ("texture", 0.10), ("photography", 0.05)]


def pick(rng, options, legal=None):
    pool = [o for o in options if legal is None or o in legal] or list(options)
    return rng.choice(pool)


def weighted(rng, pairs):
    r, acc = rng.random(), 0.0
    for value, w in pairs:
        acc += w
        if r <= acc:
            return value
    return pairs[-1][0]


def resolve(rng, domain, sid):
    school_name = rng.choice(list(SCHOOLS))
    s = SCHOOLS[school_name]
    surface_name = rng.choice(s["surfaces"])
    surface = SURFACES[surface_name]

    legal_dialects = [d for d, v in DIALECTS.items()
                      if school_name not in v["forbid_schools"]]
    # A dialect is a flavour, not the default: most specimens are canonical.
    dialect_name = None if rng.random() < 0.7 else rng.choice([d for d in legal_dialects if d])
    dialect = DIALECTS[dialect_name]

    media_mode = weighted(rng, MEDIA)
    recipe_harmony = pick(rng, HARMONY)
    caps = set(s["caps"]) | surface["caps"] | dialect["caps"]
    # Colour is a capability like any other, and the one round 1 measured as the
    # top complaint (92%). Every recipe names a ground key; anything but an
    # achromatic scheme also names a hue the card has no way to request.
    caps.add("palette_key")
    if "achromatic" not in recipe_harmony:
        caps.add("accent_hue")
    if media_mode == "photography":
        caps.add("photo_background")
    if media_mode == "texture":
        caps.add("texture")

    recipe = dict(
        id=sid, domain=domain, content=DOMAINS[domain],
        school=school_name, surface=surface_name, dialect=dialect_name,
        composition=pick(rng, COMPOSITION, s["composition"]),
        type_class=pick(rng, TYPE, s["type"]),
        ratio=pick(rng, RATIO, s["ratio"]),
        hierarchy=pick(rng, HIERARCHY),
        geometry=pick(rng, GEOMETRY, s["geometry"]),
        contrast=pick(rng, CONTRAST),
        figure_ground=pick(rng, FIGURE_GROUND, s["figure_ground"]),
        ornament=pick(rng, ORNAMENT, s["ornament"]),
        harmony=recipe_harmony,
        key=pick(rng, KEY),
        density=pick(rng, DENSITY),
        media=media_mode,
        photo_bg=(media_mode == "photography"),
        caps=sorted(caps),
        dsl_gap=sorted(caps - DSL_CAN),
    )
    recipe["art"] = " ".join(x for x in (
        s["rules"], surface["rules"], dialect["rules"]) if x)
    recipe["layout"] = COMPOSITION[recipe["composition"]]
    return recipe


def main(per_domain=25, seed=11):
    rng = random.Random(seed)
    out, n = [], 0
    for domain in DOMAINS:
        for _ in range(per_domain):
            n += 1
            r = resolve(rng, domain, f"r{n:03d}")
            r["id"] = f"r{n:03d}-{domain}-{r['school']}"
            out.append(r)
    path = Path(__file__).parent / "recipes.jsonl"
    with open(path, "w") as f:
        for r in out:
            f.write(json.dumps(r) + "\n")

    import collections
    print(f"{len(out)} recipes -> {path}")
    print("schools:    ", dict(collections.Counter(r["school"] for r in out)))
    print("compositions:", dict(collections.Counter(r["composition"] for r in out)))
    print("media:      ", dict(collections.Counter(r["media"] for r in out)))
    gaps = collections.Counter(g for r in out for g in r["dsl_gap"])
    print("capabilities the DSL lacks, by frequency:", dict(gaps.most_common()))


if __name__ == "__main__":
    main()
