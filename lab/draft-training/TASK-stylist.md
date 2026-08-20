# TASK: Stylist draft — can heavy draft training improve the BASE model's card 美学?

## The question (from the human, verbatim intent)
Can we improve Qwen3.8's output aesthetics (美学) in the L0 vertical by fine-tuning
the DRAFT model — i.e., enhance base-model quality in a vertical via a heavily
trained draft?

## Physics first
Under exact-match verification (what production serves), the draft cannot change
one output byte — only speed. This experiment must (a) demonstrate that with a
byte-identity control arm, and (b) test the ONE mechanism by which a draft CAN
change outputs: **lenient (lossy) acceptance** — accept a draft token when it is
"close enough" to the target's choice (in target top-k, or within logprob margin
τ of top-1). Under lenient acceptance the draft's learned taste leaks into the
output. Hypothesis to TEST, not assume: a draft trained only on the *best* cards
+ lenient verify steers generation toward the target's own high-quality mode.
A clean, measured negative is a fully successful outcome. Do not oversell.

## Plan

### 1. Score the harvest cards (on-box only; production server as judge — it's just inference)
For each of the ~1,096 card outputs (pick + compose families in
`/home/ubuntu/qwen38-h200/harvest/out.jsonl`, minus the 177 duplicate-query
drops): base-model-as-judge score 1–10 for DESIGN quality only (visual
hierarchy, theme usage, imagery, section richness, information density — not
correctness), temp 0, short scoring output. Also compute cheap structural
metrics (theme present, image count, distinct section types, token length).
→ `src/stylist/scores.jsonl`. All outputs are the model's own, so self-judge
bias is symmetric; we only need a RANKING. If 1,096 judgments are too slow,
subsample intelligently but keep full coverage of the compose family.

### 2. Train the stylist draft
Same pipeline as the card draft; REUSE `/mnt/dflash-feats` (features are valid —
same sequences, same trajectories). Training set: top-quartile cards by judge
score + the existing general slice (keep it — forgetting protection). Initialize
from the TRAINED card draft (`models/Qwen3.8-27B-DFlash-trained`), not 0e6412a.
If the quartile (~275 seqs) is too small to train stably, use score-weighted
sampling over all cards instead of a hard cut — note which you chose and why.
~8 epochs, same hyperparameters as the successful run.

### 3. Lenient verify — serving-side, research copy only
Offline replay CANNOT do this: cached hidden states are only valid along the
exact-verify trajectory; the first accepted divergence invalidates them. So
patch acceptance in a research copy of the dflash worker (you know it well):
accept draft token when it is in target top-k (try k=2 and k=3) OR when its
logprob is within τ of top-1 (pick τ from the parity study's margin data).
All patches live under `src/stylist/`; NEVER touch production defaults or
`launch_qwen_ab.sh`. ONE 27B server at a time — follow the sequential-swap
discipline recorded in PROGRESS.md (the outage lesson); restore production and
health-check after every window.

### 4. Three arms, ~40 held-out card/combo queries, temp 0
- **A**: exact verify — baseline outputs (any draft; outputs are target-defined).
- **B**: stylist draft + exact verify — MUST be byte-identical to A. This is the
  physics control. If it differs, there is a bug: STOP and fix before arm C.
- **C**: stylist draft + lenient verify (each k/τ variant) — the only arm
  allowed to differ.

### 5. Measure
- Divergence: % tokens changed, % outputs changed (C vs A), where in the card
  the divergences occur (structure vs values vs style fields).
- Quality: blind pairwise judge A vs C with the base model, BOTH presentation
  orders (position-bias cancel), win/tie/loss; structural-metric deltas.
- Validity: C outputs still parse as L0 cards (text-level checks — sections
  well-formed, balanced braces, known section types; no renderer on box).
- Speed: tok/s per arm (lenient should be faster — higher acceptance; report it).

### 6. Deliverable
`STYLIST.md`: divergence rates, judge win-rate, validity rate, speed, 3 concrete
diff examples (A vs C snippets), and a verdict with a recommendation
(ship / don't ship / what result would change the answer). Log to PROGRESS.md
as you go, same style as before.

## Hard rules (unchanged from CLAUDE.md)
No git pushes. No external APIs. No phone changes. No production-default
changes. Never leave the box without a healthy serving container on :30878.
