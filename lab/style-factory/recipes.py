#!/usr/bin/env python3
"""Sample the style space into concrete recipes.

Axes are deliberately coarse — a recipe is an ART DIRECTION, not a pixel spec.
The mockup model interprets it; the judges hold later stages to the mockup.
Deterministic: same seed, same recipes, so the batch is resumable by id.
"""
import itertools
import json
import random
from pathlib import Path

DOMAINS = {
    "weather": "a weather app screen: city name, current temperature hero, condition, a 7-day forecast list with lo/hi per day",
    "news":    "a news reader screen: section label, one lead headline with points/author/comments meta, five ranked story rows with title and points/author meta",
    "stock":   "a stock market screen: one lead ticker with price and signed % change, five market-mover rows with ticker, name, price, signed change",
    "reading": "a reading-list screen: section label, one featured saved article title, five saved-article rows with title and points meta",
}

GENRES = [
    ("cinematic-photo", "full-bleed atmospheric photograph background, frosted dark glass panels, hairline numerals", True),
    ("material-light",  "light grey page, white cards with soft shadows and 12px corners, one bold accent, clean sans", False),
    ("dense-feed",      "warm-white background, dense compact divider rows, no cards, bold titles, high information density", False),
    ("editorial-serif", "pure white, enormous whitespace, elegant serif display type, hairline dividers, museum restraint", False),
    ("dark-terminal",   "near-black charcoal, aligned mono-style numerals, thin glowing accent lines, trading-desk austerity", False),
    ("glass-vibrant",   "saturated gradient background, translucent white glass cards, soft glow, playful modern", False),
    ("newspaper",       "off-white paper texture feel, black serif masthead type, thin double rules, broadsheet dignity", False),
    ("pastel-soft",     "soft pastel background, rounded 20px cards in complementary pastels, friendly rounded sans", False),
    ("brutalist",       "stark white, oversized black grotesque type, thick black rules, raw grid, no decoration", False),
    ("neon-night",      "deep navy-black, one electric neon accent, dark elevated cards, subtle glow on key numbers", False),
]

FONTS = [
    "Roboto with hairline-thin display numerals",
    "Roboto Medium titles with Regular body",
    "elegant Georgia-like serif display with small sans meta",
    "condensed bold sans display, tabular numerals",
]

LAYOUTS = [
    "hero block top-left, list panel anchored at the bottom",
    "centered hero, full-width list below",
    "list-only: no hero, six equal rows fill the screen",
    "magazine: oversized lead occupying the top half, compact rows below",
]

ACCENTS = ["indigo", "google-blue", "vermilion red", "forest green", "amber", "electric cyan", "magenta"]
DENSITY = ["airy", "regular", "dense"]

def main(n_per_domain: int = 30, seed: int = 7):
    rng = random.Random(seed)
    space = list(itertools.product(GENRES, FONTS, LAYOUTS, ACCENTS, DENSITY))
    out = Path(__file__).parent / "recipes.jsonl"
    with open(out, "w") as f:
        i = 0
        for domain, brief in DOMAINS.items():
            for (genre, gdesc, photo), font, layout, accent, dens in rng.sample(space, n_per_domain):
                i += 1
                f.write(json.dumps({
                    "id": f"s{i:03d}-{domain}-{genre}",
                    "domain": domain, "content": brief,
                    "genre": genre, "art": gdesc, "photo_bg": photo,
                    "font": font, "layout": layout, "accent": accent, "density": dens,
                }) + "\n")
    print(f"{i} recipes -> {out}")

if __name__ == "__main__":
    main()
