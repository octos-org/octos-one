# Layout gates — built and measured

Deterministic layout checks over renderer geometry. No model call, no pixels,
about one second per card end to end.

The pixel-based predecessors are in `../style-factory/FINDINGS-cheap-gates.md`.
They failed, and one failed while appearing to work: a screenshot carries
appearance, not intent, so a lone numeral and a text column broken into single
characters are the same tall block of ink. The renderer knows the difference and
always did.

## Measurement

60 cards from the corpus — **one per model family**, all 60 realizing to
distinct trees — each rendered twice: as written, and with exactly one injected
defect. Six defect classes, ten cards each.

| gate | caught its own defect | fired on unmutated cards |
|---|---|---|
| squeeze | 10/10 | 0/60 |
| truncated | 10/10 | 0/60 |
| contrast | 10/10 | 0/60 |
| offscreen | 10/10 | 4/60 † |
| overflow | 10/10 | 7/60 † |
| overlap | 6/10 ‡ | 0/60 |
| tap_target | — | 60/60 (see below) |
| clipped, sliver | not injected | 0/60 |

**57/60 mutants caught, 53/60 unmutated cards left alone.** Both residuals were
checked by hand rather than assumed, and both go the same way:

**‡ The four uncaught mutants never contained the defect.** The overlap injector
sets a row's flow to Overlay; on four cards the render changed but no two
siblings ended up sharing pixels. Every intersecting pair in those four is one
of L0's two intentional stacking patterns — a scrim over an `Image` or
`MapView`, and a transparent `Button` over the content it makes tappable — which
the gate exempts correctly. Effective recall on mutants that actually carried
their defect is **57/57**.

**† The seven flagged "clean" cards are genuinely broken.** "Clean" meant
"not mutated by me", not "defect-free":

| card | model | defect |
|---|---|---|
| 020 | news | `Semiconductor Headlines` — Label **388px** wide in a 307px box, on a 360px screen |
| 053 | news | `Cybersecurity Headlines` — Label **363px** in 307px |
| 036 | travel-zurich | `Good day to be outside` — Label **347px** in 307px |
| 041 | activity | `Things to Do in Naples` — Label **340px** in 307px |
| 028, 054 | stock-news | `Semiconductor News` — Label **324px** in 307px |
| 034 | travel-lisbon | a View whose right edge sits **38px** past its parent's |

Five distinct headline labels overflow their card by 17–81px and four run off
the screen entirely. These are shipping corpus cards; nothing had caught them.

So: **0 confirmed false positives in 60**, which bounds the true rate at **4.9%**
(95% one-sided, `1 − 0.05^(1/60)`) — not zero, because 60 samples cannot buy
zero. The pixel predecessor's "clean on six specimens" bounded nothing below
39%, and measured ~85% false when the sweep was widened.

### What the first run got wrong

Worth recording, because it is the same mistake this project keeps making. The
first attempt drew 50 cards from the 263 `# model: weather` cards and scored
96%/100%. Those 50 realized to **byte-identical DSL** — the corpus varies theme
and palette, and the theme axes are not in the shipping build. It was one sample
counted fifty times. `make_samples.py` now spreads across model families and
rejects any tree it has already seen.

The mutations were wrong too. Picking a random site and setting `width: 26` is
inert on a two-character label; `flow: Overlay` is inert on a container that was
already stacked. Every inert mutation was being charged to the gate's recall.
They are now guaranteed by construction — `width: 20` on a run of ≥12
characters, `height: 8` on a container with children, ink set to exactly the
ground colour — and `measure.py` drops any pair that still renders identically.

## `tap_target` is correct and does not discriminate

It fires on every render because the defect is on every render: **350 of 400
tappable nodes across the clean cards measured under 48dp**, mostly 279×37 (a
forecast row, 11dp short) and 40×33 (a chip).

L0 wraps tappable content in `View{flow: Overlay}` + `Button{width: Fill
height: Fill}`, so the hit box is exactly the content box and inherits its
height. Nothing pads it to the minimum. That is a real accessibility defect in
the renderer, uniform across the corpus — and a demonstration that a gate can be
correct and still carry no discriminating power. Keep it, report it separately,
do not let it gate.

## `truncated` — solved by counting glyphs

This was the open gap, and asking the framework does not close it. Makepad's
layouter *does* have an `is_truncated`, but for a stock `Label` it is
**hardcoded false** — `layout_multiline` only computes it when you opt into
`max_lines` or an ellipsis (`draw/src/text/layouter.rs:301`), and the one path
that does compute it drops the value unread (`draw_text.rs:2164`). Every other
framework's equivalent flag has the same shape of hole: Flutter's
`didExceedMaxLines` is documented unreliable and always false on web; Android's
`Layout.getEllipsisCount` misfires under RecyclerView reuse.

The signal that does work is already in the draw buffer. **The draw call emits
one quad per glyph**, and `Area::Instance.instance_count` is that number. A run
that painted 11 quads for a 28-character string stopped early.

Probed directly:

| case | box | glyphs | non-space chars | |
|---|---|---|---|---|
| unconstrained | 195×14 | 31 | 31 | fits |
| 60px box | 48×63 | 26 | 26 | **wrapped**, nothing lost |
| container height 20 | 189×14 | 30 | 30 | clipped 1px, nothing lost |
| width 80, `max_lines: 1` | 73×12 | **11** | 28 | **truncated** |
| width 80, ellipsis | 78×14 | **13** | 29 | **truncated** |

Spaces paint nothing, so the comparison is against non-space characters.
Ligatures and emoji sequences collapse and undercount — `office fluffy
difficult` paints 17 glyphs for 22 letters. Measured across **1109 real text
runs** the benign floor was **0.75** (`To office`, one ffi ligature) with a
median of 1.00; real truncation measured **0.30–0.45**. `GLYPH_FLOOR = 0.6`
sits in that gap.

Result: **10/10 recall, 0/60 false positives**, and it fires on none of the
other five defect classes.

This also settles what "truncation" means here: makepad *wraps* by default and
only drops glyphs under `max_lines`, so a narrow column produces stacked
characters (caught by `squeeze`) while a pinned line produces lost characters
(caught by `truncated`). They are different defects with different gates.

**Still worth doing**: capture the authoritative flag too. Four lines — store
`text.is_truncated || size_in_lpxs.width > max_width` on `DrawText` at
`draw_text.rs:2164`, read it in `geometry_json`. Two independent signals that
must agree is stronger than either alone.

## `clipped` never fired

An undersized container in makepad does not clip its children — they paint
outside it. The symptom of "height too small" is **overflow**, not clipping, so
the clip gate stayed silent while overflow caught every case. Kept for the
containers that do set a clip rect.

## How it works

`WidgetTree::geometry_json` (in the makepad fork at
`aichat/widgets/src/widget_tree.rs`) walks every laid-out node and emits its
rect, its *clipped* rect, its text, and its ink and fill colours. It is wired to
`MAKEPAD_DUMP_GEOMETRY` in `app/app/src/main.rs`, after the draw, and is inert
unless that variable is set.

Two supporting fixes were needed and are worth knowing about:

- **`Area::rect_union`** (`aichat/platform/src/area.rs`) — `Area::rect` reads
  only the first instance, which for a `Label` is its first **glyph**. A text
  run reported as 10×13 when it is really 210×13 makes every geometric check
  nonsense. The union across `instance_count` fixes it.
- **`MAKEPAD_WINDOW_SIZE`** — the desktop build opens 900×700 landscape, and a
  card laid out at that shape is a different card.

## Running it

```bash
python3 make_samples.py 60                     # realize, bake sources, inject one defect each
python3 render.py samples/good samples/bad     # ~1s per sample
python3 measure.py -v                          # recall and false-positive rate per gate
```

`gates.py` runs standalone on any geometry dump:

```bash
MAKEPAD_SEED_CARD_FILE=card.dsl MAKEPAD_WINDOW_SIZE=360x780 \
  MAKEPAD_DUMP_GEOMETRY=/tmp/g.json octos-app
python3 gates.py /tmp/g.json
```

`synth_data.py` builds a data snapshot for any card from its own `source`
declarations, so cards from any model family realize without a network fetch.

## What this does not show

- **One shape, one theme.** All 120 renders are 360×780 dark. Other widths and
  the light and photo palettes are unmeasured, and width is exactly the axis the
  overflow findings above turn on.
- **Synthetic defects.** Injected at the DSL level, not mistakes a generator
  actually made. Real generator errors may not look like these — though the six
  corpus defects the gates found were nobody's injection.
- **Sources are baked to literals.** A live fetch renders em dashes on a desktop
  with no proxy, and a card of em dashes has no text to squeeze. Long place
  names and other locales are untested, and those are what push a label past its
  container.
- **No aesthetic claim.** These catch defects. Whether a card that passes them is
  any good is a different question, and its answer is a paired comparison, not a
  score.

## The rule these serve

```
if any gate FAILs:  reject, do not score
else:               ask the judge which of A and B is better
```

Never average a hard defect with soft aesthetics. That is how a catastrophically
broken card and a good one both scored 2/10.
