# Cheap aesthetic gates: what transfers, and what does not (2026-08-30, revised 2026-08-29)

There is real literature on computationally checkable interface aesthetics —
Ngo et al.'s fourteen measures (balance, equilibrium, symmetry, density,
simplicity, …), later simplified to seven, and Aalto Interface Metrics, which
computes seventeen from a screenshot or URL. Four of those map directly onto
defects found by hand this session, so they looked like free wins.

**Three of the four do not work as naively implemented.** Measured against two
known-good renders and one known-bad:

| gate | broken quake card | quake exemplar (good) | shipped weather (good) | verdict |
|---|---|---|---|---|
| contrast | 18.3:1 pass | 20.4:1 pass | **1.0:1 fail** | wrong region |
| alignment | 5 edges | **10 edges** | 3 edges | inverted |
| dead space | 24% | 15% | 11% | no separation |
| palette | 1 hue | 4 hues | 2 hues | no separation |

## Why each failed

**Contrast samples the wrong thing.** By hand it caught three real bugs — a card
that had merged into its page (separation 0), a bar at 1.76:1 on navy, a hero at
2.59:1 over a photograph — but each time I chose the two regions myself: this
bar against that panel. Automatically it picks the most common ink and the most
common background across the whole screen, which on `ship_light` lands in the
app's dark green chrome and reports 1.0:1 against itself. **Contrast is a valid
gate that requires element segmentation first**, which is precisely why AIM
segments the page before computing anything. Skipping that step does not
approximate it.

**Alignment is inverted.** The premise — fewer distinct left edges is more
disciplined — is wrong for card layouts. The good exemplar has TEN because it is
a legitimate multi-column list (magnitude, place, time); the broken card has
five. A metric that scores the good render worse than the bad one is not
mis-tuned, it is measuring the wrong property.

**Density and palette do not separate.** Both pass everything at any threshold
that does not also fail the good renders. With three samples that is unsurprising
and not yet evidence either way.

---

# Revision: the gate that "worked" does not work either

The first version of this document closed by saying `layout_lint.py` — a text
block taller than it is wide has been squeezed — separated cleanly, one failure
on the broken card and clean on six known-good renders, and that it stays.

That claim was wrong. Three things came out of widening the evidence.

## 1. Six clean specimens are worth almost nothing

Zero false positives in six observations bounds the true false-positive rate, at
95% one-sided confidence, only at

    1 − 0.05^(1/6) ≈ 39%

To bound it under 1% takes about 299 clean observations. Six renders never had
the power to support "it separates cleanly"; the number was reported as if it
did.

## 2. Swept over every render on disk, it is ~85% false

78 renders were judged (105 PNGs on disk; 31 too small or unsegmentable). The
gate fired on 15 of them. Inspecting each flag:

| flagged shape | ×  | what it actually is |
|---|---|---|
| 14×27, 16×26, 16×27, 20×27 | 9 | a **single numeral** — a list index. A digit is taller than wide. |
| 52×55 | 2 | a **weather icon** — sun behind cloud. Not text at all. |
| 12–16% of width | 3 | narrow labels and glyph columns |
| 106×159 | 2 | the broken quake hero (the same render, twice) |

The rule assumes every block of ink is a run of text. Numerals, icons, and
badges are all legitimately taller than wide.

## 3. The one true positive fires for the wrong reason — and misses its twin

The broken quake was flagged on a 106×159 block. That block is not three stacked
characters. It is **one glyph**: the digit `3`. The hero magnitude `3.9` had been
split so the `3` sat alone in a 106px column, and the rule caught "a big digit is
taller than wide" — which is indistinguishable from a legitimate hero numeral.

Worse, the sibling render `fid/r096-quake-memphis.png` was counted as one of the
six clean specimens. It has the **identical defect** — same lone `3`, same
`12 km NW / of La / Romana,` wrapping four words to a line. It passed only
because the `3` and the first text line happened to share a scanline, so the row
scan merged them into one 490×142 block that is wider than tall.

The separation was a segmentation coincidence in both directions.

## What survives

**Contrast, once fed segmented blocks.** Re-run per text block — ink inside the
block against the ring just outside it, blocks under 12px tall dropped (that is
the bezel seam, black against black, which produced the original 1.0:1):

| render | blocks | worst block contrast |
|---|---|---|
| r096-quake broken | 14 | 8.1:1 |
| r096-quake sibling | 14 | 8.1:1 |
| quake_exemplar | 13 | 6.6:1 |
| ship_dark | 12 | 7.3:1 |
| ship_glass | 12 | 5.9:1 |
| **ship_photo** | 11 | **3.8:1** |
| ship_light | 11 | 8.1:1 |

Every value is now plausible, the 1.0:1 artefact is gone, and the single
sub-4.5:1 result is the white hero over a photograph — a defect confirmed by
hand earlier this session. Segmentation was the whole missing ingredient, as the
first version of this document predicted.

**Alignment, repaired but non-discriminating.** Clustering *block* left edges
instead of per-scanline first-ink gives 2–4 clusters everywhere: broken 4, good
2–4. Sane at last, but it does not separate. Report it; do not gate on it.

## Where the segmentation should come from — and it is already built

Not from the pixels. The renderer already knows every rectangle it drew, and
reconstructing that from a raster is what produced every failure above.

**The facility exists in the makepad the app links against.** `app/app/Cargo.toml:24`
points `makepad-widgets` at `aichat/widgets`, and there:

    WidgetTree::snapshot(&self, cx: &Cx) -> Vec<WidgetSnapshot>   aichat/widgets/src/widget_tree.rs:1955

`WidgetSnapshot` (`aichat/platform/studio/src/studio.rs:280`) carries
`id, widget_type, window_id, visible, enabled, x, y, width, height, text, value,
checked, selected` — with window offsets already applied — and derives `SerJson`,
so serialising it is free. It is installed onto `Cx` automatically the moment a
`Root` widget is applied (`widgets/src/widget.rs:1274`), which octos-app does at
`app/app/src/main.rs:3715`. No opt-in.

Every L0-emitted widget reaches it: `Splash::register_view_subtree`
(`aichat/widgets/src/splash.rs:5014`) eagerly inserts its evaluated subtree into
`cx.widget_tree()`. Rects survive outside the draw pass — `Area::rect(cx)`
(`platform/src/area.rs:271`) reads the retained instance buffer, gated only on
`draw_list.redraw_id`, and `Area::clipped_rect` gives what is actually visible.

The hook point is three lines from the existing capture call. `main.rs:9939`
already runs `monitor::arm_capture(round)` immediately before `cx.redraw_all()`,
with `cx` and `self.ui` both live:

```rust
let widgets = cx.widget_tree().snapshot(cx);   // after the draw, not inside it
```

That pairs a PNG and its exact geometry from the same frame. Two gaps: L0 nodes
are mostly anonymous (`l0_widgets.rs:897` writes a bare `Widget{`; the fix is a
counter-minted `l0n{k} :=` prefix, mirroring how `l0map{n}` is already minted at
`:880`), and `color` is not in the snapshot (add it via
`DrawVars::get_instance_on_area`, `platform/src/draw_vars.rs:344`).

With that, the checks stop being statistics and become invariants:

| defect | gate |
|---|---|
| overlap | `\|Mi ∩ Mj\| / min(\|Mi\|,\|Mj\|) > 0` for unrelated siblings, minus declared overlays |
| clipping | `1 − \|Mi ∩ clip_i\| / \|Mi\| > 0` where clipping is forbidden |
| truncation | shaped grapheme count vs input count — the only way to know `3` lost its `.9` |
| spacing rhythm | sibling gaps must land on a declared spacing token, `min_s \|g − s\| ≤ ε` |
| alignment | declared peer anchors within a row/column, counting *violations* not distinct edges |
| touch target | hit rect in logical units ≥ 48×48 dp — never inferrable from visible ink |

Plus a metamorphic check that needs no sidecar semantics at all: **render the
same card at nominal and enlarged text scale and diff the node topology**. This
is dVermin's method (ASE '22, [arXiv:2212.04388](https://arxiv.org/abs/2212.04388)),
which reported 97% precision and recall on issue-page detection by comparing two
renders instead of judging one.

## What must not be gated

Palette harmony, typographic taste, whitespace and density, hierarchy, emphasis,
balance, symmetry, simplicity, brand fit, perceived clutter, overall polish.
Global alignment counts, colour counts, entropy, edge density, and Ngo-style
aggregate scores are descriptive statistics, not defect detectors — which is
what the top table of this document measured.

And the composition rule that names the failure already observed, where a
catastrophic layout error and a good layout both scored 2/10: **never average a
hard defect together with soft aesthetics**. Evaluate lexicographically —

    if any hard invariant fails: reject, do not score
    else: invoke the aesthetic judge

## Prior art worth reading rather than reimplementing

- [Owl Eyes](https://arxiv.org/abs/2009.01417) (ICSE '20) and
  [Nighthawk](https://arxiv.org/abs/2205.13945) — screenshot in, display-issue
  region out; 0.84 precision / 0.84 recall. Deep-learning, research-grade.
- [dVermin](https://dl.acm.org/doi/10.1145/3551349.3556935) (ASE '22) — the
  differential-scale method above.
- [UIED](https://github.com/MulongXie/UIED) — classical CV segmentation of GUI
  screenshots. Emits elements, not defects; text path needs OCR.
- [Google's Accessibility Test Framework for Android](https://github.com/google/accessibility-test-framework-for-android)
  — consumes the view hierarchy, already covers touch-target size, labels, and
  contrast. Direct evidence that the structure-first path is the right one.
- [UIClip](https://arxiv.org/abs/2404.12500) (UIST '24) — a CLIP model trained on
  2.3M UIs with synthetic defects, scoring design quality from a screenshot. This
  is the model-based alternative to everything above, and the closest existing
  thing to the judge octos-one already calls.
- [pixelmatch](https://github.com/mapbox/pixelmatch) / odiff — golden-image diff.
  Strictly stronger than any aesthetic metric *when a baseline render exists*.

## Status

`aesthetic_gate.py` — not shipped, four gates, three invalid, one salvageable
only after segmentation.

`layout_lint.py` — **downgraded from gate to probe.** It is a squeezed-text
heuristic with a measured ~85% false-positive rate on the corpus available, and
it missed a defect identical to the one it caught. It must not gate the beauty
loop in its current form.

The next attempt starts from renderer geometry, not from pixels — and does not
claim separation again until it has been measured on enough renders to support
the claim.
