# The beauty loop, second design (2026-08-30)

What changed, and why each change has a measurement behind it.

## 1. Score fidelity to the mockup, never absolute quality

The absolute 1-10 is **flat between 5 and 7**, which is where all real work
happens. Five rounds of genuine fixes moved it 5.25 -> 5.25 while paired judging
went 4-0. It cannot be a target.

Fidelity has a real ceiling (10 = matches the target) and returns a `gaps`
sentence naming concrete differences. Those sentences are the work queue —
"warm ivory ground became cool gray" is testable; "5/10" is not.

## 2. Judge paired, always

87% self-agreement, measured. It detected every real change this session while
the absolute number sat still.

## 3. Screen the mockup BEFORE grading against it

Two independent questions, and only the second discriminates:

| mockup | image quality | buildable |
|---|---|---|
| stock-swiss | 8/10 | **8/10** |
| weather-punk_zine | 9/10 | **3/10** |
| weather-art_deco | 9/10 | **2/10** |

Image quality is uniformly high — the generator produces clean, legible,
coherent designs. **Buildability is what varies**, and it varies enormously.

Grading a 2/10-buildable design against the native renderer measures nothing.
That is why art_deco sits at 1.88 and never moves: it is a routing failure, not
a rendering one.

## 4. Buildability is the ROUTER, not a filter

Low-buildability designs are not rejected — they go to the **webview**
(`runhtml`) path, which has no such vocabulary limit and already ships in the
app. The corpus's existing `dsl_gap` labels are the routing key.

    buildable >= 6   ->  native L0 card, graded on fidelity
    buildable <  6   ->  webview card, graded on fidelity separately

This keeps the webview as a genuine delivery path rather than a benchmark that
has already answered its question. Both paths ship; only the measurement is
split, so a native score is no longer contaminated by designs nothing native
could ever reach.

## 5. Verify wiring before measuring the idea

The lesson that cost a week. Before asking "does capability X help?", build a
deliberately garish version, screenshot it, count the pixels. No pixels, no
experiment.

The accent axis failed this test after a week of experiments that assumed it
passed: it sets `l0_accent`, which nothing reads, and never touches the ground
or the ink. Stroke fails it too — and that is how we know it is broken upstream
rather than mis-specified.

## 6. Genericity gate

How many of the 967 corpus cards consume the capability, before building it.
`l0_bar` shipped at 16% and returned a null.

## Order of work

**Phase 0 — unblock the corpus.** All 100 cards fail to parse on the shipping
build: their theme lines carry axes that were never merged. Merge the axes
(opt-in, byte-identical when unused, 150 tests pass) or strip them — but
stripping changes what is measured, which produced a confounded result today.

**Phase 1 — screen and route all 100 mockups.** Two scores each, then split the
corpus into a native set and a webview set. Cheap: no rendering, no generation.
This alone probably explains most of the corpus's dead weight.

**Phase 2 — fix the palette control.** Rebind `accent` to `l0_base`, `l0_fill`
and `l0_text` with contrast held. Pixel-gate that the page ground actually
changes. Then re-measure fidelity on the native set. If it moves, the theme-axes
line was starved rather than useless; if not, it is dead on evidence.

**Phase 3 — the cycle.** Per capability: wiring gate, genericity gate, build,
paired fidelity on the specimens whose `gaps` name it, keep only if it wins.

## Cost

About 2 minutes per specimen (render + judge). No image generation — the 100
mockups already exist, cost real money, and are not reproducible. No HTML
regeneration in the loop; the webview arm renders only for the designs routed
to it.

## Known ceiling

`sdf.stroke` does not draw in this Makepad build — bisected to completion,
design-time and runtime. More than half the corpus names stroke among its
requirements, so those specimens route to the webview until it is fixed
upstream. Worth knowing before starting: the native set is smaller than the
corpus.
