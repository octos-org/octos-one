# STYLIST.md — can a heavily trained draft improve the base model's card 美学?

**Short answer: no, and we now know exactly why.** Under the verification rule
production uses, it is impossible by construction. Under the one rule that makes
it possible — lenient acceptance — it is possible but it makes cards *worse*, and
the reason is measurable: when this draft disagrees with the target, it is
usually not narrowly wrong, it is catastrophically wrong.

Everything below was measured on this box on 2026-08-20. Code is in
`src/stylist/`, raw data in `/home/ubuntu/qwen38-h200/draft-training/stylist/`,
running log in `PROGRESS.md`. **Production was never taken down for this
experiment.**

---

## 0. What was built

| step | artefact | result |
|---|---|---|
| 1 | `score_cards.py` | 1096 pick+compose cards judged for design, 446 s, 0 unparsed |
| 2 | `make_splits.py`, `run_train.sh` | stylist draft: 200 best cards + 360 general, 8 epochs, 695 s |
| 3 | `patch_lenient.py`, `test_lenient.py` | lenient-verify research worker, unit-tested against sglang's exact rule |
| 4 | `run_arms.py` | 8 arms x 40 held-out queries, temp 0 |
| 5 | `analyze_arms.py`, `pairwise_judge.py`, `pick_tau.py` | divergence, validity, blind win rate, speed |

---

## 1. The physics claim could not be tested as specified — the stack is not deterministic

The brief required arm B (stylist draft + exact verify) to be **byte-identical**
to arm A (card draft + exact verify), as proof that a draft cannot change output.
It is not: **4/40 identical**.

That looks like a falsification until you run the control for the control:

> **Arm B2 = arm B run again. Same draft, same config, same server, back to
> back. 5/40 byte-identical, 23.6% of tokens changed.**

Two runs of *the same thing* diverge as much as two different drafts do. The
serving stack is nondeterministic at temperature 0 — batch- and cache-dependent
reduction order in the target's own kernels, nothing to do with speculation.
This retroactively explains the 146/177 duplicate-query divergences recorded
during feature extraction on 2026-08-19.

The noise floor is strongly slice-dependent, which matters for everything below:

| | tokens changed, B vs B2 (same config, repeated) |
|---|---|
| cards (shorter outputs) | **6.5%** |
| unseen combos (long compose outputs) | **26.5%** |

One early flip cascades through a long generation; short cards mostly reproduce.

**Attempted fix, failed.** `--enable-deterministic-inference`, three launches,
all crash with `Buffer overflow when allocating memory for batch_prefill_tmp_v
with size 2415919104 ... only 2147483648 bytes available`. The request is a fixed
2.25GB against a 2GB flashinfer workspace and did not move when
`--chunked-prefill-size` went 8192 -> 4096 or `--context-length` 65536 -> 57600.
It also force-disables the radix cache, which would cost a full 53k prefill per
query. Abandoned.

**So the physics claim is reported in the only form this stack supports:** not
"arm B is identical to arm A", but "**arm B differs from arm A by no more than
arm B differs from itself**" — which is what the measurements show (28.5% vs
23.6% tokens changed, 10% vs 12% identical). The exact-verify accept rule itself
is separately proven target-defined by `test_lenient.py`, which checks our
implementation against `compute_dflash_correct_drafts_and_bonus` on 200 random
batches: accept lengths, bonus tokens and the committed prefix all match.

---

## 2. The stylist draft: different taste, identical competence

The obvious way this experiment could have been vacuous is if the stylist draft
were simply the card draft again — it starts from that checkpoint and trains on a
*subset* of that checkpoint's own data. It is not.

| | card draft | stylist draft |
|---|---|---|
| acc@48 cards | 41.28 | **41.27** |
| acc@48 unseen combos | 39.91 | **39.81** |
| acc@48 general | 27.50 | **28.10** |
| token accuracy on held-out card slots | 75.06% | 74.46% |
| **proposals differing from the card draft, card slots** | — | **7.01%** |
| card blocks containing at least one differing proposal | — | **52.2%** |
| relative L2 weight distance | — | **2.60%** |

So: **equal competence, genuinely different function.** Under exact verify all
7% of those disagreements are discarded and the output cannot move. Under lenient
verify they are exactly the substrate that leaks in. That is the experiment.

**Selection note.** The judge's global top quartile is **188 pick / 12 compose** —
it rates single-app weather cards far above composed ones, so "best cards" and
"pick cards" would have been the same set, and the stylist draft would have
differed from the card draft in card *family* rather than in taste while the eval
set is compose queries. Selection was therefore stratified within mode:
**120 pick + 80 compose** (cut at SCORE 91 and 88) plus the whole 360-sequence
general slice.

---

## 3. Why lenient acceptance was always going to be a bad trade

Measured on 1983 real rejections (`pick_tau.py`), at the slot where exact verify
first rejects the draft:

| draft token's rank in the target's distribution | share |
|---|---|
| rank 2 | 14.5% |
| rank 3 | 20.2% cumulative |
| ranks 4-10 | 35.6% cumulative |
| ranks 11-100 | 59.2% cumulative |
| **rank > 1000** | **20.6%** |

| logit gap, target top-1 minus draft token | share |
|---|---|
| < 1.0 | 3.8% |
| < 2.0 | 6.5% |
| **>= 6.0** | **88.7%** |

**When this draft is wrong it is usually catastrophically wrong.** Only one
rejection in seven is a near-miss. So a "close enough" rule that is loose enough
to fire often is necessarily loose enough to commit genuinely wrong tokens. This
single table predicts every result in §4 before any of them were run.

`tau` was picked from this data, not guessed: the target's own top1-top2 gap has
median ~3.5 and is under 1.0 at only 20% of slots, so **tau = 2.0** fires only
where the target is genuinely near-indifferent. `k = 2` and `k = 3` per the
brief; **`k = 10` added beyond the brief** to bound the effect from above.

---

## 4. The arms

40 held-out queries (24 unseen-combo, 16 card), temp 0, single stream, one
research server, `--speculative-num-draft-tokens 16`, `--speculative-draft-window-size 4096`.
Reference is arm B (stylist draft, exact verify) — same weights, only the rule
differs.

| arm | rule | tok/s | mean tokens | **valid** | identical to B | tokens changed | hit 4096 cap |
|---|---|---|---|---|---|---|---|
| A | card draft, exact | 250.7 | 1569 | 0.95 | 10% | 28.5% | 0 |
| **B** | **stylist, exact (reference)** | 256.5 | 1549 | **0.95** | — | — | 0 |
| **B2** | **B repeated — noise floor** | 259.0 | 1442 | **0.95** | **12%** | **23.6%** | 0 |
| B3 | stylist, exact via the lenient code path | 273.0 | 1558 | 0.95 | 10% | 28.8% | 0 |
| **C tau=2.0** | lenient, logprob margin | **306.7** | 1434 | **0.95** | 3% | **40.7%** | 0 |
| C k=2 | lenient, top-2 | 223.7 | 1567 | **0.17** | 0% | 54.5% | 1 |
| C k=3 | lenient, top-3 | 234.0 | 2556 | **0.03** | 0% | 77.8% | **18** |
| C k=10 | lenient, top-10 | 383.8 | 3017 | **0.00** | 0% | 93.6% | **25** |

Validity is a text-level check: fenced as `runl0`, `# level: L0` present, a
`view root` exists, braces/brackets/parens/quotes balanced outside strings, no
dangling view references, no widget names absent from the corpus vocabulary. All
four exact arms sit at exactly 0.95 — the same two records, both *genuine
refusals* of a request no app in the catalogue covers.

### Divergence is real, and it is not noise

On the **cards** slice, where the noise floor is only 6.5%, arm C k=2 changes
**55.7%** of tokens — 8.6x the floor. Lenient acceptance unambiguously moves the
output. The question is only which direction.

### It moves it down

Changed lines land overwhelmingly on structure (3243 lines) and style (1857)
rather than values (1126) — the draft is not swapping city names, it is rewriting
the card. Structural means, relative to arm B:

| metric | B2 (noise) | C tau=2.0 | C k=2 | C k=3 | C k=10 |
|---|---|---|---|---|---|
| distinct widget kinds | -0.20 | **-1.20** | **-2.83** | -5.58 | -13.38 |
| events (interaction) | -0.65 | **-1.51** | -1.68 | -3.01 | +8.44 |
| state declarations | -0.45 | -0.72 | -1.52 | -2.33 | -3.42 |
| images | -0.05 | -0.25 | -0.45 | -0.53 | -0.50 |
| has loading state | 0.00 | 0.00 | -0.07 | -0.23 | -0.88 |

Every lenient arm loses widget variety, interaction and states. None gains any.
The `+8.44` events and `+109` lines at k=10 are not richness — they are
repetition loops (see §6).

### Blind pairwise judging, both presentation orders

A win counts only if it survives order reversal; an order-dependent preference is
scored as a tie.

| pair | C/arm wins | A wins | tie |
|---|---|---|---|
| **B2 vs A — noise floor** | 4 (10%) | 18 (45%) | 18 |
| C tau=2.0 vs A | 4 (10%) | 24 (60%) | 12 |
| C k=2 vs A | **0 (0%)** | **36 (90%)** | 4 |

**Read the noise floor first.** Two arms that differ only by nondeterminism are
judged 18-4 for A. The judge is not neutral: **the longer card of the pair wins
82-86% of the time**, and arm A happened to run longer. So the tau=2.0 result
(24-4) is only modestly worse than noise once length is accounted for, and on the
length-matched subset it is 0 wins / 4 losses / 5 ties (n=9) against the floor's
3 / 6 / 14 (n=23).

The k=2 result needs no such correction: **mean output length is identical
(1569 vs 1567 tokens) and A still wins 36-0.** That is a real, large quality
loss.

---

## 5. Speed

Lenient acceptance was predicted to be faster. It is not, except where it is
failing.

`B3` is the control that makes this readable: exact semantics running through the
same Python accept path as the C arms, so implementation overhead is held fixed.
Against B3's 273.0 tok/s:

- **tau=2.0: 306.7 tok/s (+12%)** — a real, modest speed win, and the only arm
  that gains speed without breaking anything.
- **k=2: 223.7 tok/s (-18%)**, k=3: 234.0 (-14%). *Slower despite accepting
  more.* Lenient acceptance pushes the sequence onto a trajectory the draft was
  never trained on, so downstream acceptance falls further than the extra accepts
  gain.
- **k=10: 383.8 tok/s (+41%)** — the fastest arm and the worst. Server logs during
  it read `accept len: 16.00, accept rate: 1.00, gen throughput 795 tok/s`: the
  draft's proposal is inside the target's top-10 at every slot of every block
  because both models have fallen into a repetition loop they agree on
  confidently.

Absolute tok/s here is lower than FINDINGS.md's 210.8/261.5 because the research
server shared the card with a live (shrunk) production container. Only the
arm-to-arm comparison is meaningful.

---

## 6. Three concrete diffs (arm B vs arm C)

**(a) tau=2.0 — valid, but poorer.** `travel-00590`, "compose a Dublin dashboard".
The lenient card silently deletes the entire find/search interaction:

```
B: state q     { shape: text, initial: "Dublin travel guide" }
   state typed { shape: text, initial: "" }
   state editing { shape: enum[none, find], initial: .none }
   event open_find  { editing: set(.find), typed: clear }
   event typing     { typed: set($value) }
   event run_find   { q: set($value), typed: clear, editing: set(.none) }
   event close_find { editing: set(.none), typed: clear }
C: (all seven lines absent)
```
It also drops `sun`, `moon`, `parks` and `cafes` sources, cutting 1914 tokens to
1288. Perfectly well-formed runl0. Just a thinner card.

**(b) k=2 — breaks bindings and the fence.** `dashboard-01015`, "a foodie card for
Athens":

```
B:  ```runl0                                    C:  ```
    source place sys.geocode(name: state.city)      source place sys.geocode(name: "Athens")
```
The language tag is gone (so it no longer parses as an L0 card at all) and the
city is hardcoded instead of bound to state — the card can no longer be
re-pointed at another city.

**(c) k=2/k=3 — degenerate loops.** `dashboard-00947`, "tech pulse":

```
B: symbols: "NVDA,AMD,AVGO,SMCI,MU,TSM,MRVL,ARM,CRWV,PLTR,SNOW,AI,VRT,ANET,ORCL,MSFT,GOOGL,META"
C: symbols: "NVDA,AMD,AVGO,SMCI,SMH,INTC,ARM,MRVL,ON,MPWR,TER,ADSK,SNPS,CDNS,ALAB,CRDO,DELL,HP,
             IBM,ORCL,MS,GOOGL,AMZN,GOOG,AMZN,MS,GOO,GOOG,AMZN,MS,GOOG,AMZN,MS,GOOG,AMZN,MS,GOOG, ...
```
It never terminates the string. At k=3, 18/40 generations hit the 4096-token cap;
one ends as several hundred repetitions of the single character `8`.

**`protect_eos` does not prevent this.** It demands exact agreement only where the
target's *own* top-1 is a stop token; a trajectory that has already diverged
simply never arrives at one.

---

## 7. Verdict

**Do not ship lenient verification.** Not at k=2, k=3 or k=10 — those destroy the
cards (validity 0.17 / 0.03 / 0.00, and 90-0 against baseline in blind judging at
matched length). Not at tau=2.0 either: it is the one setting that stays
well-formed and it is genuinely 12% faster, but it never wins on quality (4 wins
against 24 losses, 0 wins on the length-matched subset) and it systematically
strips interaction and widget variety out of the cards. There is no operating
point where output quality goes up.

**The direct answer to the question asked.** You cannot improve Qwen3.8's card
aesthetics by training the draft. Under production's exact verify the draft
provably cannot change a byte — it is a pure speed device, and the card draft
already delivers 1.79x for free. Lenient acceptance is the only lever that
touches output, and this measurement says the lever is attached to the wrong
thing: it does not import the draft's taste, it imports the draft's *errors*,
because 88.7% of the draft's disagreements are not near-misses at all.

**What result would change this answer.** One number: the rejection-margin
distribution in §3. Lenient acceptance only makes sense with a draft whose
disagreements are near-ties — say, over half of rejections at rank 2-3 and a
median logit gap under 1.0, against today's 14.5% and >= 6.0. That is a different
and much harder training objective than acceptance rate: it asks the draft to be
*calibrated* where it is wrong, not merely right more often. Distillation on the
target's full distribution (KL on soft targets) rather than cross-entropy on hard
labels is the obvious thing to try, and it is a research programme, not a config
change.

**If the goal really is better cards, the draft is the wrong lever entirely.**
The target writes every token that survives verification. Prompt, catalogue and
few-shot exemplars decide what it writes; the draft only decides how fast. The
one genuinely useful by-product of this experiment points the same way: the
design judge is largely a length-and-widget-variety detector (Spearman +0.72 with
distinct widget kinds, +0.51 with length; it picks the longer card 82-86% of the
time), so before optimising anything for "design quality", get a metric that
measures design rather than size.

---

## 8. Caveats, stated plainly

- **The judge is weak.** On a 1-10 scale it rated essentially every card 9 even
  with a harsh rubric; re-anchored to 0-100 it spread 10..92 but still put 232 of
  1096 cards on the single value 86, and it ranks mainly by widget variety and
  length. It is adequate as a *ranking* for selecting a training quartile, which
  is all step 2 needed. It is not a trustworthy arbiter of taste, which is why
  §4's judging is reported against a measured noise floor and a length-matched
  subset rather than as a bare win rate.
- **40 queries, single stream, one run per arm.** Enough to establish a validity
  collapse from 0.95 to 0.17 and a 36-0 judging result; not enough to quote a
  precise divergence percentage. The exception is arm B2, which exists precisely
  to bound run-to-run variation, and it is large.
- **Nondeterminism bounds everything.** 23.6% of tokens change between two
  identical runs (26.5% on long compose outputs). Any claimed effect below that
  on the combos slice is unmeasurable here.
- **`protect_eos` was ON in every lenient arm**, a deliberate deviation from a
  pure "accept if close" rule, taken so the quality comparison would not become a
  comparison of truncation. It is a one-flag change to test without it, and §6
  shows it is not doing much work anyway.
- **Absolute tok/s are not comparable to FINDINGS.md.** The research server ran
  alongside a live shrunk production container.
- **The stylist draft is 8 epochs on 560 sequences with untuned hyperparameters**,
  identical to the card run's settings. Its loss barely moved (2.43 -> 2.36)
  because it starts from a checkpoint already trained on a superset of this data.
  A more aggressive style-specialisation run would produce a more divergent draft
  — and, on §3's evidence, a *worse* one under lenient verify, since the extra
  divergence would be concentrated in exactly the tokens the target rejects hard.
- **This says nothing about lenient verify in general**, only about lenient
  verify with a DFlash draft trained by hard-label cross-entropy on this
  vertical.

---

## 9. The distillation follow-up (2026-08-20): a clean negative, with the reason

§7 named exactly one experiment that could overturn the verdict:

> Lenient acceptance only makes sense with a draft whose disagreements are
> near-ties — say, over half of rejections at rank 2-3 and a median logit gap
> under 1.0, against today's 14.5% and >= 6.0. [...] Distillation on the
> target's full distribution (KL on soft targets) rather than cross-entropy on
> hard labels is the obvious thing to try.

It was tried. **It does not work, and the reason is one number measured before
any draft was trained: on this vertical the target has no soft distribution to
distil.** The tau=2.0 arm and the blind pairwise judging were therefore *not*
rerun — the brief gated them on the margins becoming near-tie-dominated, and
they did not come close. Code in `src/distill/`, data in `/mnt/dflash-teach`,
log in `PROGRESS.md`. **Production was never taken down** and never left the
launcher defaults.

### 9.1 The target is already a one-hot

KL and cross-entropy are the same objective up to the teacher's own entropy:
`KL(p_T || p_S) = CE(p_T, p_S) - H(p_T)`. So the first thing to measure is
`H(p_T)`. Teacher-forcing all 1319 usable harvest sequences through the live
production server and reading back `input_top_logprobs` gives the target's top-64
distribution at **1,661,323 positions** (0 errors, 0 token mismatches, 1814 s,
836 MB — pure inference, so no outage window was needed at all).

| the target's own distribution, at completion positions | |
|---|---|
| mean top-1 probability | **0.9640** (median 1.0000) |
| positions with top-1 probability > 0.99 | **85.4%** |
| mean entropy | **0.1081 nats** (median 0.0002) |
| positions with entropy < 0.01 nats | **79.7%** |
| mean number of tokens above 1% probability | **1.39** |
| mean top1-top2 logit gap | **10.63** (median 11.75) |
| top-1 probability by mode | pick **0.996** / compose **0.981** / general 0.835 |

On the pick cards this vertical is actually about, `H(p_T) ~ 0.008 nats`. The
"soft targets" are hard labels with rounding error. This was visible in the very
first training step: `ce 6.583929538726807` against `kl 6.581383466720581` — the
whole difference between the two objectives, 0.0025 nats.

### 9.2 Three arms, all replicating the shipped card draft exactly

Every run copies `ckpt_long`'s saved hyperparameters verbatim (init 0e6412a,
`splits`, 8 epochs, anchors 32, span 1536, accum 2, lr 1e-4, warmup 60, block
sizes {8,16,32,48}, W=4096, fp8-KV) so **the loss is the only difference**. That
is verified, not assumed: step-1 cross-entropy is bit-identical across all four
runs at 6.583929538726807.

- **T=1, alpha=1.0** — the literal §7 proposal. Full-vocabulary KL: the 64
  teacher entries carry their true probabilities and the rest of the vocabulary
  is lumped into one residual symbol matched against the student's own residual
  mass, so mass the draft puts where the target has none is punished.
- **T=2 and T=8, alpha=0.7** (+0.3 CE) — because at a median top1-top2 gap of
  11.75 logits, only a large temperature carries the target's *rank ordering*
  into the loss, which is what §7 is really asking for. Both use Hinton's `T^2`
  rescaling; without it a T=8 run is a T=1 run with the soft term turned down
  64x, which is how the first T=8 launch was found to be miscalibrated and was
  restarted. At `T != 1` the tail's logits are unknown, so those two are
  restricted to the teacher's top-64 and renormalised.

`src/distill/test_kl.py` checks the loss against brute force: the lumped tail is
exact when the teacher's tail is empty and a lower bound otherwise, `KL(p||p)=0`
at T=1 and T=4, and off-support mass is punished hardest (copycat 0.000 <
near-miss 6.375 < off-support 37.693).

### 9.3 §3, re-measured — offline, and validated against the server probe first

§3 was measured by instrumenting the lenient worker on a research server: one
production window per draft, one draft per launch. It can be done offline
instead, **exactly**. Under exact verify nothing the draft proposes survives, so
the committed trajectory *is* the recorded temp-0 generation; replaying it
reproduces the serving trajectory token for token, and at the first rejected slot
every preceding slot matched the target's own choice, so the target's
distribution there is its true distribution at that position — which is what the
teacher data holds.

Run on the **stylist draft**, the same draft §3 was measured with:

| | §3, server probe (1983 rejections) | offline (26,436 rejections) |
|---|---|---|
| rank 2 | 14.5% | **15.2%** |
| cum rank 3 | 20.2% | **21.9%** |
| cum rank 10 | 35.6% | **37.5%** |
| gap < 1.0 | 3.8% | **3.6%** |
| gap < 2.0 | 6.5% | **6.7%** |
| gap >= 6.0 | 88.7% | **85.9%** |

Same table, 13x the statistics, no research server and no window. The one thing
it cannot resolve is rank beyond the teacher's top-64, which lands in a `rank>64`
bin (43.6%) rather than §3's `101-1000` + `>1000` (40.8%). As a by-product,
`teacher top1 == the recorded token` at **99.18%**, which independently
re-validates the teacher-forcing premise the whole feature pipeline rests on.

### 9.4 The result

157 held-out sequences, ~27-30k rejections per arm.

| | card draft (CE) | **KL T=1** | KL T=2 | KL T=8 |
|---|---|---|---|---|
| rank 2 | 15.4% | **16.3%** | 11.8% | 15.9% |
| cum rank 3 | 22.0% | **22.8%** | 16.4% | 21.8% |
| cum rank 10 | 37.2% | **38.0%** | 26.2% | 33.7% |
| **rank > 64** | 43.4% | **42.5%** | 62.2% | 53.5% |
| gap < 1.0 | 3.7% | **4.4%** | 3.1% | 3.2% |
| gap < 2.0 | 6.8% | **8.0%** | 5.7% | 6.0% |
| gap >= 6.0 | 85.9% | **84.4%** | 88.1% | 86.6% |
| mean target prob of the rejected token | 0.0202 | **0.0239** | 0.0174 | 0.0186 |
| accept / verify | 5.83 | 5.82 | 5.21 | 4.87 |

On the **cards** slice alone — the vertical the project is about — it is starker:
`gap >= 6.0` is **91.9 / 90.5 / 93.1 / 91.6%** and `cum rank 3` is
**20.9 / 21.7 / 14.6 / 21.5%**.

Acceptance, same gates as the card run:

| | card draft (CE) | KL T=1 | KL T=2 | KL T=8 |
|---|---|---|---|---|
| cards >= 40 | 41.28 | **41.33** | 40.73 | 40.02 |
| unseen combos >= 25 | 39.91 | **39.98** | 39.03 | 38.23 |
| general >= 8 | 27.50 | 26.56 | 25.36 | 24.09 |
| seam windows (cards) | 38.83 | **38.89** | 37.81 | 36.33 |

**The best arm is the one with almost no soft signal in it.** T=1 KL is a
statistical tie with cross-entropy on every axis — it moves `cum rank 3` by
+0.8 points and `gap >= 6.0` by -1.5 — because, per §9.1, it *is* cross-entropy
to within 0.008 nats on cards. Both temperature arms, the ones built to give the
hypothesis its best shot, make the error distribution **worse** and cost
acceptance.

§7's bar was "over half of rejections at rank 2-3 and a median logit gap under
1.0". The best arm reaches **22.8%**, and its median gap is still **above 6.0**
(84.4% of its rejections exceed 6.0). The lever's reach is essentially untouched:
tau=2.0 would newly accept 6.8% of rejections with the CE draft and 8.0% with the
distilled one.

### 9.5 The control that makes this a result and not a failed run

Mean held-out `KL(target || draft)` at *every* slot, not just rejections
(~490k slots per arm):

| | card draft (CE) | KL T=1 | KL T=2 | KL T=8 |
|---|---|---|---|---|
| all | 2.002 | **1.979** | 2.228 | 2.487 |
| cards | 1.180 | **1.165** | 1.378 | 1.643 |
| unseen combos | 1.599 | **1.576** | 1.847 | 2.120 |
| general | 4.115 | **4.048** | 4.320 | 4.539 |

The T=1 run **did** optimise its objective — it holds the lowest held-out KL on
every slice. It beats cross-entropy by 1.1%, because cross-entropy was already
almost optimising it. The temperature arms optimised a *different* objective
(softened, support-restricted) and are correspondingly worse at the real one.
So this is not "distillation failed to train"; it is "distillation trained, and
the property it was supposed to buy did not appear."

### 9.6 Why the two mechanisms cancel

The `rank > 64` row is the whole story. At `T != 1` the teacher's tail logits are
unknown, so the KL is restricted to the top-64 and off-support mass is only
defended by the 0.3 cross-entropy term — and rank>64 proposals jump from 43.4% to
53.5% (T=8) and 62.2% (T=2). The T=1 arm's lumped-tail term is exactly what
prevents that, and it is also the arm with essentially no soft signal left to
learn from.

**On this data the soft signal and the tail defence trade off against each other,
and the direction that has signal is the direction that loses the defence.** A
draft can be taught to rank the target's head correctly, or to keep its mass off
the target's tail, but the training signal available here does not do both.

### 9.7 What this changes

**Nothing in §7's verdict, and one thing in §7's list of open questions.** Do not
ship lenient verification. The draft remains a pure speed device under
production's exact verify, and the card draft's 1.79x still stands. §7 offered
distillation as "the obvious thing to try [...] a research programme, not a
config change"; it has now been tried at three points of the obvious knob, with
the exact-replication control, the offline §3 measurement validated against the
server probe, and the held-out-KL control. It is closed.

**The reason it is closed generalises past the objective.** Calibrating a draft's
errors requires the target to *have* calibrated errors to imitate, and at
temperature 0 on templated L0 cards the target is a deterministic function with
mean entropy 0.008 nats. There is nothing there to learn. Any future attempt at
lenient acceptance on this vertical would have to manufacture the soft signal —
sampling the target at temperature, or training against a different notion of
"acceptable" than the target's own next-token distribution — which is a different
project from drafting, and would still have to beat the fact that the target
writes every surviving token anyway.

**What would change the answer now.** Not a better draft objective. Only a domain
where the target is genuinely uncertain — note `general` mode, where top-1
probability averages 0.835 against cards' 0.996, is the one slice where lenient
acceptance has any raw material at all (its rejections sit at `gap < 2.0` 12.8%
of the time versus cards' 4.0%). If lenient verification is ever worth revisiting,
it is there, not here.

### 9.8 Caveats

- **The offline §3 replay cannot resolve rank past 64.** Everything beyond lands
  in one bin, with a lower bound on its logit gap (top-1 minus the 64th entry)
  used so the `>= 6.0` share stays honest. It matched the server probe on the
  bins both can see; it was not separately re-validated on the distilled drafts,
  which is a gap only a research-server window would close.
- **`mean gap` is a lower bound**, for the same reason.
- **Three temperatures is not a search.** T=2 came out worse than T=8, so the
  degradation is not monotone in T and no clean "trend" is claimed — only that
  every point measured is worse than cross-entropy. Nor were alpha, learning
  rate, epoch count or teacher K tuned; K=64 is justified by the top-64 holding
  a mean 0.99972 of the mass, not by a sweep.
- **The T=1 and temperature arms differ in three ways at once** (temperature,
  alpha, and support treatment), because the T=1 arm is the literal §7 proposal
  and the others are the best-shot variants. T=2-vs-T=8 isolates temperature; the
  T=1 comparison does not.
- **Teacher logprobs came from the production server**, whose numerics
  (mrr=4, NGRAM, chunked prefill 32768) differ slightly from the extraction
  server that produced `/mnt/dflash-feats`. Teacher targets only need to be the
  target's distribution; the `99.18%` top-1 agreement with the recorded tokens
  bounds any inconsistency.
- **No serving-side measurement was made of the distilled drafts.** By the
  brief's own gate that was the right call, but it means the end-to-end tok/s and
  output-divergence numbers in §4-§5 were not re-derived for them.
