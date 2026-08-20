# PROGRESS

Running log for the compositional DFlash draft project. Newest entries appended.

## 2026-08-19 05:40 — session start

- Harvest running in tmux `harvest`, 266/1496 done, errs=0, ~25 tok/job pace
  (~20 jobs / 25 s => ETA ~25 min). Watching per Goal 1.
- Box state at start: GPU 96GB total, **86.6GB used by `qwen-ab`, only 10.1GB free**.
  Disk 3.8TB free, host RAM 385GB available.
- `qwen-ab` is serving with **NGRAM** speculation (draft 32 / sam 31), *not* DFLASH.
  Confirmed from `launch_qwen_ab.sh`. So the 0e6412a draft checkpoint is idle and
  free for us to load; all eval must be offline (accept-length simulator), which
  is what Goal 4 asks for anyway.
- Host python has **no torch / transformers**. All GPU work must run inside the
  serving image `ominix-sglang-hopper:0.5.16-qwen38` (has sglang + torch +
  transformers). Decision: run training/extraction as `docker run` from that image.

### Decisions
- **D1**: Work tree lives in `/home/ubuntu/draft-training` (git repo). Checkpoints go
  to `/home/ubuntu/qwen38-h200/draft-training/ckpt/` as CLAUDE.md specifies
  (created on demand, outside git — they are multi-GB).

## 2026-08-19 05:58 — Goal 2 done (CONDITIONING.md), pipeline written

### Goal 2 findings that change the plan
- **The draft never embeds context tokens.** Its context KV is materialized from
  the *target's* hidden states at layers `[1,16,31,46,61]` (residual-stream inputs,
  i.e. outputs of layers 0/15/30/45/60), concatenated to `[N, 25600]` and pushed
  through the trainable `fc` + `hidden_norm` before each draft layer's `kv_proj`.
  Full derivation with line cites in `CONDITIONING.md`.
- **CLAUDE.md's mask token is wrong.** There is no `<|MASK|>` in the tokenizer and
  248077 is not an added token. The served id is **248070** (`<|audio_start|>`,
  repurposed) per `dflash_config.mask_token_id`. Using 248077 would have silently
  destroyed conditioning.
- **"draft 32 / sam 31" is NGRAM, not DFLASH.** Production serves
  `--speculative-algorithm NGRAM`; the DFlash block size in effect is the
  checkpoint's `block_size: 16`, so max accepted drafts per verify is **15**.
- The user query is the **last ~5 tokens of a 53,127-token prompt**, so a bounded
  draft window is enough to copy query-derived values. This makes training
  tractable.

### Decisions
- **D2**: train with a **4096-token context window** of target hidden states
  (exactly contains the draft's own 2048 sliding window for layers 0-3;
  layer 4 gets 4096 instead of 53k). Recommend serving with
  `--speculative-draft-window-size 4096` so train and serve conditioning are
  *identical*, not merely close.
- **D3**: extract features by **teacher-forcing** prompt⧺recorded-completion
  through a DFLASH sglang server whose `dflash_worker_v2.py` is bind-mount
  patched to dump aux hidden states. One prefill per sequence instead of ~110
  decode steps, and it is exact (CONDITIONING.md §5). The 53k prompt is shared,
  so radix caching makes each sequence cost ~1.8k tokens of prefill.
- **D4**: draft reimplemented in plain PyTorch (`src/dflash_torch.py`) because
  sglang's copy is `@torch.no_grad` and writes K/V into a paged pool. Parity with
  the real stack will be checked token-for-token via dumped draft proposals.
- **D5**: optimizer is a small custom AdamW with **bf16 params/grads/exp_avg and
  fp32 exp_avg_sq** (~17.4GB). `torch.optim.AdamW` would put `exp_avg_sq` in bf16
  where squared grads underflow to zero.

### Observation worth keeping (harvest, unplanned)
Compose-mode generations take **~35-53 s** each vs **~2-5 s** for pick-mode, at
similar lengths (~1.3k-2.1k tokens). That is ~50 tok/s vs ~800 tok/s — the NGRAM
cache collapsing on compositions, measured on production traffic shape. This is
the project's motivating number and it fell out of the harvest for free.

### Status
- Harvest at 544/1496 (05:58). Compose is much slower, ETA ~07:30-08:00.
- Written and syntax-clean: `src/dflash_torch.py`, `src/dataset.py`,
  `src/make_stats.py`, `src/eval_accept.py`, `src/train_dflash.py`,
  `src/extract_hidden.py`, `src/assemble.py`, `src/launch_extract.sh`,
  `src/instrument/patch_worker.py` (+ generated patched worker).

## 2026-08-19 06:10 — model validated, teacher-forcing premise validated

### `src/dflash_torch.py` smoke test (GPU, in the serving image)
- 0e6412a loads into the reimplementation with **58/58 tensors and nothing left
  over** — 1.730B params, 3.22 GB bf16.
- Grouped multi-anchor forward == per-anchor forward to **1.5e-6 relative** in
  fp32 (bf16 differs by ~4e-3 relative purely from SDPA reduction order, which is
  why the check runs in fp32).
- Leakage probe passes: zeroing context rows at/after an anchor changes that
  block's output by exactly 0, and later blocks by a lot. Blocks cannot see the
  tokens they must predict.
- backward reaches all 58 params; peak HBM for a 24-block/4096-window step is
  **7.33 GB**.

**Bug this caught (would have been a silent train/serve mismatch):** draft layer 4
is `full_attention`, so when several anchors share one context tensor the later
anchors' layer 4 saw context reaching back to the *first* anchor's window, not
their own. Fixed by adding `cfg.ctx_window` — a per-anchor visibility window that
is the exact analogue of serve-time `--speculative-draft-window-size`
(`worker:607-623`). Before the fix grouped and solo differed by 1.6e-2 relative
*in fp32*, i.e. a real logic error, not rounding.

### Teacher forcing is exact (CPU check, `src/check_tokenization.py`)
- chat-templated prompt length matches the server's `prompt_tokens`
  **150/150 pick and 68/68 compose, delta 0**.
- retokenized completion + EOS matches `completion_tokens` **150/150 pick,
  64/68 compose** (the 4 are off by one token).
- shared prompt prefix: **53,115 of 53,127 tokens (pick)** and
  **53,184 of 53,211 (compose)**. Only the last ~12-27 prompt tokens vary per
  request, so radix caching makes extraction cheap and the shared prefix's
  hidden states need to be captured exactly twice.
- `transformers` 5.12.1 `apply_chat_template` returns a `BatchEncoding`
  (a `UserDict`, **not** a `dict`), whose `len()` is 2. Cost an hour of confusion;
  `chat_ids()` now handles it.
- Confirms CONDITIONING.md §2: `<|MASK|>` is absent from the tokenizer,
  `<|audio_start|>` is 248070, and `len(tokenizer) == 248077` — CLAUDE.md's
  "248077" is the tokenizer *length*, which is one past the last valid id.

### Decisions
- **D6**: extraction will run with `XSTEPS=1` and
  `--speculative-draft-window-size 4096`, so every request also yields one real
  DFLASH decode step. `src/verify_parity.py` compares sglang's own draft
  proposals against ours block-for-block. Same window on both sides or layer 4
  would not be comparable.

## 2026-08-19 06:30 — copy analysis (`src/copy_analysis.py`), CPU only

Measured on 40 pick + 40 compose harvested cards, 800 anchors each, block 16
(so 15 draftable slots). Full-sequence search, external corpus included.

| drafter | pick | compose |
|---|---|---|
| `ngram_no_prompt` — corpus + own output only | **14.34 / 15** | **3.69 / 15** |
| `oracle_corpus_self` — perfect copier, same sources | 14.69 | 6.10 |
| `ngram_suffix` — same trie but allowed to index the request | 13.63 | **7.70** |
| `oracle_prompt` — perfect copier, request only | 13.75 | 9.88 |
| `oracle_any` — perfect copier, anything already in context | 13.75 | **10.07** |

**This reproduces production.** `ngram_no_prompt` is what the deployed
suffix-automaton actually has (external corpus + the tokens it has emitted), and
it scores 95.6% of slots on pick and 24.6% on compose — which is the ~800 tok/s
vs ~50 tok/s split the harvest is measuring right now.

Three consequences, in order of how much they change the plan:

1. **The 53k request is an unindexed copy source.** Letting the same trie index
   the prompt lifts compose from 3.69 to 7.70 / 15 with no model at all. That is
   a cheap non-ML win and belongs in FINDINGS.md as its own recommendation.
2. **Copying alone caps out at 10.07/15 on compose.** CLAUDE.md's thesis says
   "nearly all novel tokens are COPYABLE"; measured, ~33% of compose tokens are
   *not* contiguously present anywhere in the context. Those have to be
   *generated*. A learned draft can do that and a trie structurally cannot — so
   the thesis holds, but the mechanism is copy **plus** generation, and the
   headroom above a perfect copier is the larger half.
3. **Pick is already saturated at 14.34/15.** There is essentially nothing to win
   on single-app cards; the bar there is "do not regress", which reframes the
   Goal-5 `cards>=40/48` gate as a regression guard rather than a target.

### Seam windows (±10 tokens around section boundaries), same method

| drafter | pick | compose |
|---|---|---|
| `ngram_no_prompt` (deployed shape) | 12.97 / 15 | 3.65 / 15 |
| `ngram_suffix` (trie allowed to index the request) | 11.53 | 5.41 |
| `oracle_any` (perfect copier) | 11.90 | 8.90 |

Seams are harder for everything: the perfect copier loses 1.2/15 on compose and
1.9/15 on pick versus whole-card anchors. Note `ngram_suffix` degrades much more
at seams (7.70 -> 5.41) than `ngram_no_prompt` does (3.69 -> 3.65): at a section
boundary the recent suffix is generic boilerplate, so longest-suffix matching
latches onto the wrong precedent. A learned draft is not forced into that
failure mode, which is the specific claim the seam gate is testing.

## 2026-08-19 06:25 — the incumbent's score, in the gates' own units

`k_window_accept` identity (unit-tested in `src/test_accounting.py`):

    accepted-in-a-K-window  =  K  -  verifies_used

because each verify commits `accept_len + 1` tokens and the `+1` is the target's
own token. At B=16 the ceiling for K=48 is 45.

Chained simulation, same anchors as above (25 seqs/mode, 200 anchors):

| drafter | pick acc@48 | compose acc@48 |
|---|---|---|
| `ngram_no_prompt` — what production actually has | **44.69 / 48** | **32.62 / 48** |
| `ngram_suffix` — same trie, allowed to index the request | 44.32 | 38.03 |
| `oracle_any` — perfect copier | 44.50 | 42.54 |
| ceiling at B=16 | 45 | 45 |

**CLAUDE.md's "NGRAM acceptance saturates ~33/48 on cards" reproduces exactly as
the compose number, 32.62/48.** So that figure was describing the composed case,
and single-app picks are already at 44.7/48 — essentially the ceiling.

This resets what the Goal-5 gates mean:

- `cards >= 40/48` is roughly "match the incumbent averaged over pick+compose"
  (NGRAM's blend is ~38.7). It is a **regression guard**, and it is tight,
  because pick has only 0.3/48 of headroom left.
- `unseen-combos >= 25/48` is **below** what the incumbent already scores on
  compose (32.6). Clearing it is necessary but not interesting; the number to
  beat is 32.62, and the realistic target is the copy oracle's 42.54.
- `general >= 8/48` needs only 1.2 tokens/verify — a floor against catastrophic
  forgetting, not a performance target.

I will report against **both** the stated gates and the incumbent, because
passing `unseen-combos >= 25` while scoring below 32.6 would be a regression
dressed up as a pass.

## 2026-08-19 07:47 — harvest complete, Goal 1 done

`HARVEST COMPLETE: ok=1496 errs=0 total_lines=1496`. `harvest/STATS.md` written.

- pick 516 / compose 580 / general 400; **1,950,671 completion tokens**;
  0 empty, 0 missing ```` ```runl0 ```` fence, 8 general hit the 1500 cap.
- compose cards carry **10.19 mean `source` declarations vs 7.16 for pick**, and
  **3.84 top-level sections vs 0.63** — the multi-domain structure the harvest
  was designed to produce is there.
- 143 card generations (13.0%) run past 2048 tokens, i.e. past the reach of
  draft layers 0-3 for the user query.
- Wall time per generation: pick p50 ~5s, compose p50 ~40s. Same prompt, same
  lengths — the whole gap is NGRAM acceptance.

## 2026-08-19 08:00-08:12 — fitting two model servers on one GH200

This took five launches and **one ~9-minute production outage**, recorded here
because the cause is non-obvious and will bite anyone repeating it.

**`--mem-fraction-static` is a fraction of the memory FREE AT PROCESS START, not
of total GPU memory.** sglang computes
`rest = available_after_weight_load - free_at_start * (1 - mem_fraction_static)`.
With a second process already holding memory, `free_at_start` shrinks, so the
*same* fraction means something completely different. Consequences:

- `ABMEM=0.36` killed qwen-ab outright (mamba state cache resolved to 0 requests)
  and left the box with no serving container for ~9 minutes. **My error** — I
  reasoned about the fraction as if it were relative to the 96GB card. Restored
  at a verified-good `ABMEM=0.50 ABMRR=2` first, then tuned.
- qwen-ab would not start at `ABMEM=0.42 ABMRR=2` either; the binding constraint
  is the hybrid mamba/linear-attention state cache, which needs
  `5 slots per running request` at ~147MB each. Passing the launcher's existing
  `ABMAMBA` knob fixed it: **`ABMEM=0.42 ABMRR=1 ABMAMBA=5`** boots and serves,
  using 47GB and leaving 49.7GB.
- The extraction server then needed `XMEM=0.86` (of *its* 49.7GB) to get a KV
  pool of **149,212 tokens** — it must exceed 55k, since one teacher-forced
  sequence is a 53.1k prompt plus its completion. At `XMEM=0.90` with only 42GB
  free the pool came out at 49,538 tokens, which is *below* one sequence.
- Also disabled CUDA graphs and cut chunked prefill to 8192 for the extraction
  server; it is prefill-dominated so this costs nothing and saves GB.

### A real train/serve trap caught here
`--quantization fp8` **propagates to the draft model**: the first successful
launch reported `speculative_draft_model_quantization='fp8'` and
`DFLASH fused KV materialization disabled: quantized qkv_proj is not supported`.
The 0e6412a checkpoint is BF16, and we train in BF16, so serving it as FP8 would
be a silent mismatch *and* would disable the fused KV path. Fixed with
`--speculative-draft-model-quantization unquant`; the draft now loads at 3.38GB
and logs `DFLASH fused KV materialization enabled`. **This belongs in the serving
recommendation.**

### Confirmations from the live stack
- `DFLASH draft runner ready. mask_token=<|MASK|>, mask_token_id=248070` —
  the tokenizer has no `<|MASK|>`, so the id override is what is used, exactly as
  CONDITIONING.md §2 derived.
- `block_size=16, draft_window_size=4096, compact_cache=True`.
- Extraction smoke test on 6 records: prompt-token mismatches **0/6**,
  completion retokenization exact **6/6**, assembled features **9/9 clean, 0
  misaligned**, and `check_data.py` passes on the real features.
- Shared prompt prefix confirmed on real data: pick 53,115 / compose 53,184 /
  general 5 tokens.

## 2026-08-19 08:20-08:55 — extraction, parity, baseline, first training

### Extraction (Goal 3 prerequisite)
1496 sequences teacher-forced through the instrumented DFLASH server in
**12.5 minutes at 0.50 s/request** (radix caching absorbs the shared 53k prefix),
producing 91GB of target aux features.

**177 of 1496 came out unusable** and were dropped. Cause: `harvest.py`'s
`DASHBOARDS` templates that ignore `{c}` were enqueued once per city, so ~30 jobs
share a byte-identical query. The second and later ones hit the radix cache
*through the completion as well* (we teacher-force the completion as prompt), so
the target never recomputed those positions and only the divergent tail was
dumped. Only 31/177 were byte-identical to their surviving twin — the other 146
diverged, which is batch-order nondeterminism at temperature 0. Every one of the
177 is a repeat of a query that survives elsewhere, so **no unique query is
lost**; 1319 sequences and 1235 unique queries remain.

### Parity with the real serving stack — the risk CLAUDE.md flags, retired
`src/verify_parity.py` compares sglang's own DFLASH draft proposals against
`src/dflash_torch.py` on identical context, mid-card (completions truncated to
45% so the decode step is not in the trailing-EOS region):

| | blocks identical | token agreement |
|---|---|---|
| bf16 KV | 22/36 | 96.85% |
| **fp8 KV emulation** | **25/36** | **97.22%** |

**The residual is numeric, not structural.** At every divergent slot our top-1
beats sglang's token by a median logit margin of **0.0625** (max 0.1875), while
the typical top1-top2 gap across all slots is **0.6875** — an order of magnitude
larger. A wrong mask id, wrong layer taps, or an off-by-one context would produce
confident disagreements, not ties, and would never leave 2/3 of blocks matching
on all 15 slots.

**New finding: the draft's KV cache is fp8_e4m3 at serve time.** `kv_cache_dtype`
is a single global server arg — there is no draft-specific override — so the
draft's context KV is quantized even though its weights are bf16. Emulating it
(straight-through) improved parity, and training now does the same via `--kv-fp8`.

### Goal 3 dry run — passes
100 sequences, mixed block sizes {8,16,32,48}: no OOM, **peak 26.26 GB** (gate is
<=30GB), loss avg50 6.05 -> 5.57.

### Goal 4 baseline — the existing draft is far behind the incumbent
Existing `0e6412a` general draft, block 16, W=4096, fp8 KV, 867 windows:

| slice | acc/verify | acc@48 |
|---|---|---|
| pick | 0.85 | 21.68 |
| compose | 0.73 | 19.92 |
| general | 0.55 | 16.65 |

Against the NGRAM incumbent measured on the **same held-out sequences**
(`ngram_no_prompt`): cards **38.00**, unseen-combos **31.08**, general **7.71**.
So the general draft would be a large regression on cards if shipped as-is —
this is exactly the "how far does the general draft already get" question Goal 4
was asked to answer, and the answer is "not far".

### First training run (Goal 5, 1 epoch as specified)
581 steps, **177 seconds**, peak 26.31 GB, loss avg50 ~6.0 -> ~4.0.

| slice | baseline | trained 1ep | NGRAM incumbent | copy oracle |
|---|---|---|---|---|
| cards | 21.39 | **32.77** | 38.00 | 43.43 |
| unseen-combos | 19.50 | **32.64** | 31.08 | 42.55 |
| general | 18.71 | **20.25** | 7.71 | 22.14 |
| seam windows (cards) | 19.19 | **29.84** | — | — |

Gates after 1 epoch: `cards>=40` **fails** (32.77), `unseen-combos>=25` **passes**,
`general>=8` **passes**. Versus the incumbent it is ahead on unseen combos and
general but behind on cards. An 8-epoch run is under way; epochs cost 3 minutes.

### Finding that may outrank the model
`ngram_suffix` — the *same* trie, merely allowed to index the 53k request —
scores **41.02** on cards and **38.57** on unseen combos, against the deployed
`ngram_no_prompt`'s 38.00 / 31.08, and close to the copy oracle's 43.43 / 42.55.
That is a larger gain than one epoch of draft training bought, with no model,
no GPU, and no train/serve risk.

## 2026-08-19 08:55-09:45 — training, evaluation, end-to-end benchmark

### Training (Goal 5)
- 1 epoch (the brief): 581 steps, 177s, peak 26.31GB. Cleared unseen-combos and
  general gates, missed `cards>=40` at 32.77.
- 8 epochs: 4648 steps, 1150s, peak 26.31GB, loss avg50 6.58 -> ~2.0.
  **All three gates pass** (cards 41.28, unseen combos 39.91, general 27.50) and
  every slice beats the NGRAM incumbent measured on the same held-out sequences.
- Block size is not a lever: the 1-epoch checkpoint re-evaluated at block 32 and
  48 moved cards acc@48 by +0.5 / +0.8 only. Acceptance is limited by where the
  draft first errs.

### End-to-end benchmark
Exported the checkpoint to a model dir (`src/export_draft.py`) and served it with
flags identical to production apart from the speculation stack. Decode
throughput, TTFT excluded, held-out queries, single stream:
**cards 1.46x, unseen combos 2.03x, general 1.99x, overall 1.79x.**

### Second production outage (~12 min), and its cause
Launching the DFLASH server while `qwen-ab` was up killed `qwen-ab`'s scheduler
(`running_phase_sigquit_handler` -> SIGQUIT, i.e. a CUDA OOM in the scheduler
process). `--mem-fraction-static` reserves weights + KV pool but **not** prefill
activations; two 27B servers each chunk-prefilling a 53k prompt cannot both keep
that headroom on one 96GB card. The fix was to benchmark the two arms
**sequentially**, which is also the fairer comparison since each then runs with
production-identical flags. Production was restored to launcher defaults
(`mem_fraction_static=0.75, mrr=4, NGRAM, draft 32, corpus yes`) and verified
healthy after each swap.

### Final box state
- `qwen-ab` **up on :30878 at stock launcher defaults**, no env overrides.
- No other containers running. `launch_qwen_ab.sh` never edited.
- Artifacts: `harvest/STATS.md`, `CONDITIONING.md`, `FINDINGS.md`,
  `/home/ubuntu/qwen38-h200/draft-training/{ckpt,ckpt_long,splits,*.json}`,
  trained draft dir `models/Qwen3.8-27B-DFlash-trained/`,
  features `/mnt/dflash-feats` (91GB), raw dumps deleted.

---

## 2026-08-20 06:10 — TASK-stylist session start

New question from the human: **can a heavily trained draft improve the base
model's card 美学 in the L0 vertical?** Physics says no under exact verify — the
draft cannot change one output byte — so the experiment is built around the one
mechanism that *can* change outputs, **lenient (lossy) acceptance**, with a
byte-identity control arm to prove the physics claim rather than assert it.

Box state at start: `qwen-ab` up on :30878 (stock launcher defaults, 20h
uptime), 79.1GB HBM used / 17.7GB free, no other containers. Features from the
card run still on disk at `/mnt/dflash-feats` (1319 usable sequences) and reused
verbatim — same sequences, same trajectories, so they are still valid.

### Plan of record (5 pieces, all under `src/stylist/`)
1. `score_cards.py` — judge all 1096 pick+compose cards for DESIGN quality with
   the production server, temp 0, plus model-free structural metrics.
2. `make_splits.py` — top-quartile-by-score cards + the whole general slice.
3. `patch_lenient.py` — research copy of `dflash_worker_v2.py` with lenient
   accept; `test_lenient.py` proves it against sglang's exact rule.
4. `run_arms.py` — arms A/B/C over ~40 held-out queries.
5. `analyze_arms.py` + `pairwise_judge.py` — divergence, validity, speed, blind
   win rate.

### Decisions
- **S1: the judge scores 0-100, not 1-10.** On a 1-10 scale the model rated
  essentially every card 9 (spread 5..9, and 9 for 7 of 9 probe cards) even with
  an explicitly harsh rubric and calibration bands — self-judging its own
  outputs, it will not use the low end. Re-anchored to 0-100 the same probe set
  spread 58..92 across 7 distinct values. Still coarse (the model snaps to a
  lattice around 82/88/91/92) and reported as a caveat, but usable as a
  *ranking*, which is all step 2 needs.
- **S2: the lenient rule is driven by a control FILE, not an env var.** The
  patched worker re-reads `/control/lenient.json` at 4 Hz, so arms B, C(k=2),
  C(k=3) and C(tau) all run inside ONE server launch. Two 27B servers do not fit
  on this card (the outage lesson) and every launch costs a production window,
  so the only arm that needs its own launch is A, whose *draft weights* differ.
- **S3: lenient accept is a strict superset of exact accept.** Exact matches are
  always accepted; `mode:"exact"` returns `None` from the config reader so the
  stock code path runs untouched. `test_lenient.py` checks on 200 random batches
  that top-k with k=1 and tau=0 reproduce
  `compute_dflash_correct_drafts_and_bonus` token-for-token (accept_len, bonus
  and the committed `out_tokens` prefix), that accepts are monotone in k and in
  tau, and that tau=inf accepts the whole block. All pass.
- **S4: `protect_eos` defaults ON.** Where the target's own top-1 is a stop
  token (248046/248044) the rule demands exact agreement. Without it a lenient
  accept can silently delete the target's stop token and the card runs on to the
  4096-token cap, which would turn the quality comparison into a comparison of
  truncation. Reported as a deliberate deviation, and the flag is exposed so the
  unprotected variant is one config change away.
- **S5: why lenient output stays coherent, not word salad.** The target's
  forward covers the WHOLE draft block, so `target_predict[i+1]` is already
  conditioned on the draft's tokens at slots <= i. Committing a lenient-accepted
  draft prefix plus the target's own token at the first rejection therefore
  yields a valid target continuation *of the accepted prefix*. The draft steers;
  the target still writes. This is what makes the experiment worth running.

## 2026-08-20 06:15-06:40 — step 1 (scores) and step 2 (stylist draft)

### Scoring: 1096 cards judged in 446 s, 0 unparsed
Production server as judge, temp 0, concurrency 4, 0.41 s/card. Distribution:
min 10, p25 84, median 86, p75 90, max 92, sd 10.7, **37 distinct values but
232/1096 sit on the single value 86** — the judge separates the corpus into
about six real bands, not a continuum. Pick cards mean 85.1, compose 82.5.

Spearman of SCORE against model-free structural metrics:
`n_widget_kinds +0.72`, `n_image +0.67`, `n_lines +0.59`, `n_root_sections +0.56`,
`completion_tokens +0.51`, `has_theme +0.45`. So the judge is largely ranking
**widget variety and imagery**, and only secondarily length. That is a
defensible proxy for "design richness" but it is not independent taste, and it
is recorded as the headline caveat of this experiment.

### Decision S6: select the top quartile WITHIN each mode, not globally
A global top quartile is **188 pick / 12 compose** — the judge rates single-app
weather cards far above composed ones, so "best cards" and "pick cards" are the
same set. Training on that would make the stylist draft differ from the card
draft in card *family* rather than in taste, while the eval set is compose
queries. Stratified: **120 pick + 80 compose** (cuts at SCORE 91 and 88; selected
mean 91.4 vs 85.3 for pick, 89.2 vs 83.4 for compose) + the whole 360-sequence
general slice = 560 training sequences. Hard cut, not score-weighted: 560 is
plenty to train stably, and a hard cut gives the strongest style signal, which is
what the hypothesis needs.

### Training
Init from the trained CARD draft, `splits_stylist`, otherwise identical
hyperparameters to the successful run (lr 5e-5, warmup 50, accum 2, 24 anchors,
block sizes {8,16,32,48}, W=4096, fp8-KV, grad checkpointing).
**2240 steps = 8 epochs = 695 s, peak 22.57 GB.** Ran alongside a shrunk but
LIVE production container (`gpu_window.sh open 0.42` with `ABMRR=1 ABMAMBA=5`,
47.0GB used / 49.8GB free, healthy after 80 s) — no outage.

Loss avg50 went 2.43 -> 2.36, i.e. almost flat. That is expected and not a
failure: the run starts from a checkpoint already trained on a *superset* of
this data, and the mix is now 64% general (360/560) against 30% before, so the
average is dominated by the harder slice.

### Control 1 — the stylist draft is still a competent drafter (acc@48)
| slice | card draft | stylist | delta |
|---|---|---|---|
| cards | 41.28 | **41.27** | -0.01 |
| unseen combos | 39.91 | **39.81** | -0.11 |
| general | 27.50 | **28.10** | +0.59 |

No competence was traded away, so any output divergence in arm C cannot be
explained by "the draft got worse".

### Control 2 — but it IS a different function (`draft_divergence.py`)
This was the risk that could have made the whole experiment vacuous: the stylist
run starts from the card draft and trains on a SUBSET of that draft's own data,
so the two could be the same function.

- global relative L2 weight delta **2.60%** (largest per-tensor 2.32%, all in the
  MLP up/gate/down projections of layers 1-4)
- on held-out anchors the two drafts propose a **different token at 7.01% of card
  slots**, 7.59% of unseen-combo slots, 13.12% of general slots
- **52% of card blocks** contain at least one differing proposal
- and they are equally right: token accuracy vs the recorded target is 75.06%
  (card draft) vs 74.46% (stylist) on cards

That is the experiment in one table: under exact verify all 7% of those
disagreements are discarded and the output cannot change; under lenient verify
they are exactly the substrate that can leak into the output.

## 2026-08-20 06:38-07:15 — steps 3/4: lenient verify on a research server

No production outage this time. The research server was sized to coexist with
the shrunk-but-live production container (`SMEM=0.80 SMRR=1 SMAMBA=5 SCTX=65536
SCHUNK=8192`), giving a KV pool of **77.7k tokens** — comfortably above the
53.1k prompt plus a 4k completion. `qwen-ab` answered `/health` after every
launch, kill and swap. The mamba trap from 2026-08-19 recurred exactly as
recorded (`max_mamba_cache_size=4, mamba_ratio=5 -> max_num_reqs=0`) and was
fixed the same way, with `--max-mamba-cache-size 5`.

### Measured tau/k selection (`pick_tau.py`), not guessed
The patched worker was run with `mode:"tau", tau:0.0, stats:true`, which is
exact-verify semantics with instrumentation, over 10 held-out queries = 2000
verify steps and 1983 sampled rejections. At the slot where exact verify first
rejects the draft:

| draft token's rank in the target distribution | share |
|---|---|
| rank 2 | 14.5% |
| rank 3 | +5.6% (cum 20.2%) |
| ranks 4-10 | +15.4% (cum 35.6%) |
| **rank > 1000** | **20.6%** |

| logit gap top1 - draft token | share |
|---|---|
| < 1.0 | 3.8% |
| < 2.0 | 6.5% |
| **>= 6.0** | **88.7%** |

**When this draft is wrong it is usually catastrophically wrong**, not narrowly
wrong. That single table sets the ceiling on the whole experiment: top-k is the
only lever with real reach (k=2 captures 14.5% of rejections, k=3 20.2%), and
tau is nearly inert (tau=2.0 captures 6.5%). For scale, the target's own
top1-top2 gap has median ~3.5 and is below 1.0 at only 20% of slots, so
**tau=2.0** was chosen: it only fires where the target is genuinely near
indifferent. Arms run: k=2, k=3, tau=2.0, plus **k=10** beyond the brief to
bound the effect from above.

### The physics control cannot be run as specified — the stack is not deterministic
Arm B (stylist draft, exact verify) had to be byte-identical to arm A (card
draft, exact verify). It is not: **4/40 identical**. But the control for that
control says the comparison is meaningless as stated:

**Arm B2 = arm B repeated. Same draft, same config, same server, back to back:
only 5/40 byte-identical.**

So run-to-run nondeterminism at temperature 0 is as large as any draft effect,
which retroactively explains the 146/177 duplicate-query divergences recorded
during extraction on 2026-08-19. Nondeterminism at temp 0 comes from batch- and
cache-dependent reduction order in the target's own kernels, not from
speculation.

Attempted fix: sglang's `--enable-deterministic-inference`. **Three launches,
all crash** with
`Buffer overflow when allocating memory for batch_prefill_tmp_v with size
2415919104 ... only 2147483648 bytes available in AlignedAllocator`. The request
is a fixed 2.25GB against a 2GB flashinfer workspace and did **not** move when
`--chunked-prefill-size` went 8192 -> 4096 or `--context-length` 65536 -> 57600,
so it is not derived from either. Deterministic mode also force-disables the
radix cache (logged: "radix cache is not compatible with flashinfer attention
backend for deterministic inference"), which would have cost a full 53k prefill
per query anyway. Abandoned after the third attempt; production was healthy
throughout.

**Consequence for the experiment, stated up front:** every divergence number is
reported against the measured **B-vs-B2 noise floor**, not against zero. That is
the honest form of the physics control on this stack: the claim to test is not
"arm C differs" but "arm C differs by more than repeating the same arm does".

### Decision S7: a `k=1` arm to separate the rule from its implementation
The lenient patch replaces sglang's Triton accept/bonus kernel with a Python
implementation, and the first lenient arm came back **slower** than exact
(223.7 vs 256.5 tok/s) — the opposite of the prediction. That could be the
acceptance rule or it could be the Python path. `B3_pypath_exact` runs
`mode:"topk", k:1`, which `test_lenient.py` proves is exact-verify semantics,
through the same Python path. It isolates the implementation overhead so the
rule's true speed effect can be read off.

## 2026-08-20 07:20 — first look at the lenient arms

Against arm B (stylist, exact verify) as reference, 40 held-out queries:

| arm | tok/s | outputs identical to B | tokens changed | **cards still valid** |
|---|---|---|---|---|
| B (reference) | 256.5 | — | — | 0.95 |
| **B2 = B repeated (noise floor)** | 259.0 | **12%** | **23.6%** | **0.95** |
| A (card draft, exact) | 250.7 | 10% | 28.5% | 0.95 |
| C k=2 (lenient) | 223.7 | 0% | 54.5% | **0.175** |

Two things are already clear.

1. **Divergence is real but must be read against the noise floor.** A-vs-B (28.5%
   tokens changed) is indistinguishable from B-vs-B2 (23.6%) — exactly what the
   physics predicts once the stack's own nondeterminism is accounted for. C k=2
   at 54.5% is ~2.3x the floor, so lenient acceptance genuinely moves the output.
2. **It moves it downward.** Validity collapses from 0.95 to 0.175: **29/40 C
   outputs have unbalanced braces/brackets/parens**, 8 lose the ```runl0 fence,
   5 reference views that are never declared. Structurally the cards get
   *poorer*, not richer: -2.83 widget kinds, -1.68 events, -1.52 states,
   -13.7 lines. All three exact arms sit at 0.95 validity (the same 2 records,
   both genuine refusals of an uncoverable request).

Concrete example, `dashboard-01015` ("a foodie card for Athens"), at the first
divergence — the lenient card drops the language tag off the fence and hardcodes
the city instead of binding it to state:

```
B:  ```runl0                       C:  ```
    source place sys.geocode(name: state.city)   source place sys.geocode(name: "Athens")
```

Live from the server during arm C k=10: `accept len: 16.00, accept rate: 1.00,
gen throughput 795 tok/s` — at k=10 the draft's proposal is inside the target's
top-10 at *every* slot of *every* block, i.e. the pair has fallen into a
degenerate repetition loop that both models predict confidently. Fast and
worthless; several k=3 and most k=10 generations hit the 4096-token cap instead
of stopping.

`protect_eos` does not prevent this. It only fires where the target's own top-1
IS a stop token; a trajectory that has already diverged simply never arrives at
one.
