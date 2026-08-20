# FINDINGS.md — compositional DFlash draft for Qwen3.8-27B

Everything here was measured on this box on 2026-08-19. Reproduction commands are
in `PROGRESS.md`; code is in `src/`.

**Metric.** `acc@K` = how many of the first K committed tokens came from accepted
drafts, replaying the real serving loop. The identity (unit-tested in
`src/test_accounting.py`) is `acc@K = K - verifies_used`, because every verify
commits `accept_len + 1` tokens and that `+1` is the target's own token. At block
size 16 the ceiling for K=48 is **45**. Higher is faster; `acc@48 = 0` means the
draft contributed nothing.

---

## 1. What the incumbent actually is, and why it fails on compositions

Production serves **NGRAM** speculation, not DFLASH (`launch_qwen_ab.sh`). Its
draft source is a suffix automaton over the 5-card external corpus **plus the
tokens it has already emitted**. It does **not** index the 53,127-token request.

Measured offline on the held-out set (`src/copy_analysis.py`, exact chained
replay, no GPU needed):

| drafter | cards | unseen combos | general |
|---|---|---|---|
| `ngram_no_prompt` — **what production has** | **38.00** | **31.08** | **7.71** |
| `ngram_suffix` — same trie, allowed to index the request | 41.02 | 38.57 | 8.06 |
| `oracle_any` — a perfect copier | 43.43 | 42.55 | 22.14 |

This reproduces the behaviour the harvest measured directly: single-app "pick"
cards generate in ~5 s and composed cards in ~40 s for the same prompt and
similar lengths. The corpus contains the pick templates almost verbatim
(a perfect copier scores 14.69/15 per block on pick), and contains compositions
not at all.

**CLAUDE.md's "NGRAM saturates ~33/48 on cards" reproduces as the compose
number** in the earlier whole-corpus sample (32.62). Picks are already at ~44.7,
essentially the 45 ceiling.

## 2. The thesis, corrected

CLAUDE.md: *"nearly all novel tokens are COPYABLE from the request. A learned
draft can learn that copying; a trie cannot. That is the entire thesis."*

Half of that is confirmed and half needs amending.

- **Confirmed:** the request is a large, completely unexploited copy source.
  The query sits in the *last ~12 tokens* of the prompt, so the copyable values
  are adjacent to the generation. Simply letting the existing trie index the
  request moves compose-like traffic from 31.08 to 38.57 acc@48.
- **Amended:** copying alone is **not** enough. A *perfect* copier — one that
  finds the longest contiguous match anywhere in the whole context — reaches only
  **10.07/15 slots per block on compose**. About **one third of composed-card
  tokens appear nowhere in the context** and must be *generated*. That is the
  part no trie of any design can reach, and it is the larger half of the
  remaining headroom.

So the mechanism to bet on is copy **plus** generation, not copy alone.

## 3. Train/serve conditioning — verified, not assumed

CLAUDE.md names conditioning mismatch as the classic failure. `CONDITIONING.md`
documents the serve-time construction with line cites; `src/verify_parity.py`
then checks it against reality by replaying sglang's own DFLASH draft proposals.

Mid-card (completions truncated to 45% so the probe is not in the trailing-EOS
region), our reimplementation reproduces sglang's proposals with **97.2% token
agreement, 25/36 blocks identical on all 15 slots**. The residual is numeric:
at every divergent slot our top-1 beats sglang's token by a median logit margin
of **0.0625**, against a typical top1-top2 gap of **0.6875**. A structural error
(wrong mask id, wrong layer taps, off-by-one context) produces confident
disagreements, not ties.

Three concrete traps found on the way, each of which is silent:

1. **The MASK token is 248070**, not the 248077 in CLAUDE.md. There is no
   `<|MASK|>` in the tokenizer; 248070 is `<|audio_start|>` repurposed, and
   248077 is `len(tokenizer)` — one past the last valid id.
2. **`--quantization fp8` propagates to the draft model.** The first DFLASH
   launch reported `speculative_draft_model_quantization='fp8'` and
   `DFLASH fused KV materialization disabled: quantized qkv_proj is not
   supported`. The checkpoint is BF16. Fix: `--speculative-draft-model-quantization unquant`.
3. **The draft's KV cache is fp8_e4m3.** `kv_cache_dtype` is a single global
   server arg with no draft-specific override. Emulating it in training raised
   parity from 96.85% to 97.22%.

## 4. Where the existing general draft starts

`Qwen3.8-27B-DFlash-0e6412a`, block 16, W=4096, fp8 KV, 867 windows:

| slice | acc/verify | acc@48 |
|---|---|---|
| pick | 0.85 | 21.68 |
| compose | 0.73 | 19.92 |
| general | 0.55 | 16.65 |

Against the incumbent's 38.00 on cards, **shipping the existing draft unchanged
would roughly halve card throughput**. This is the answer to the question Goal 4
was posed to settle before any training: the general draft does not transfer to
this traffic.

## 5. Result: the trained draft

`src/train_dflash.py`, initialised from `0e6412a`, 1319 usable harvested
sequences (1162 train / 157 held out), context window 4096, block sizes sampled
per group from {8,16,32,48}, fp8-KV emulation, bf16 params with fp32
`exp_avg_sq`. **8 epochs = 4648 steps = 19 minutes, peak 26.31 GB HBM.**

### Accept-length simulator, held-out sequences (`src/eval_accept.py`)

acc@48, ceiling 45:

| slice | gate | `0e6412a` | 1 epoch | **8 epochs** | NGRAM prod* | NGRAM+prompt* | copy oracle* |
|---|---|---|---|---|---|---|---|
| cards | >= 40 | 21.39 | 32.77 | **41.28 PASS** | 38.00 | 41.02 | 43.43 |
| unseen combos | >= 25 | 19.50 | 32.64 | **39.91 PASS** | 31.08 | 38.57 | 42.55 |
| general | >= 8 | 18.71 | 20.25 | **27.50 PASS** | 7.71 | 8.06 | 22.14 |

\* offline replay at block 16 for comparability; see the caveat in §7.

Accepted drafts per verify (max 15 at block 16): cards **6.72**, unseen combos
**5.44**, general **1.40** — against 0.84 / 0.70 / 0.65 for the starting
checkpoint.

**Seam windows (+-10 tokens around section boundaries), reported separately as
required:** cards **38.83**, unseen combos **37.33**, general 23.82, versus
19.19 / 18.60 / 17.00 for `0e6412a`. Seams cost about 2.5 acc@48 relative to
whole-card anchors — real but small, and the seam score alone still beats the
incumbent's whole-card score.

**The draft is not merely copying.** On general prose it scores **27.50 against a
perfect copier's 22.14**. A copy-only mechanism cannot exceed the copy oracle;
this one does, which is the capability §2 said a trie structurally cannot have.

### End-to-end throughput — the number for the ship decision

Both arms served with **identical flags** (`mem-fraction-static 0.75`,
`max-running-requests 4`, same attention/quantisation/cache backends), differing
only in the speculation stack, benchmarked one at a time on the same held-out
queries at temperature 0, single stream, **time-to-first-token excluded**
(`src/bench_serve.py`):

| slice | NGRAM (production) | DFLASH (trained) | speedup | median speedup |
|---|---|---|---|---|
| cards | 144.8 tok/s | **210.8** | **1.46x** | 1.58x |
| unseen combos | 128.5 tok/s | **261.5** | **2.03x** | 1.71x |
| general | 51.5 tok/s | **102.6** | **1.99x** | 1.91x |
| **all** | 120.7 tok/s | **215.5** | **1.79x** | — |

No slice regresses. The largest win is exactly where the project predicted it:
novel compositions.

## 6. Decisions for the human

**1. Ship as DFLASH serving? Yes — with one gate left to clear.**
1.79x overall and 2.03x on novel compositions, no slice slower, and drafts
cannot change outputs (verify is exact). What is *not* yet done: a
multi-concurrency benchmark (everything here is single-stream at
`max-running-requests 4` but one request at a time) and a soak test. Those are
cheap and should run before a cutover. Serve with:

```
--speculative-algorithm DFLASH
--speculative-draft-model-path <trained draft>
--speculative-num-draft-tokens 16
--speculative-draft-window-size 4096
--speculative-draft-model-quantization unquant     # NOT optional, see §3
```

**2. Draft window: 4096. Block size: 16.**
4096 exactly contains the 2048 sliding window of draft layers 0-3, so those
layers are numerically exact, and it bounds layer 4's view to something we can
train against — making train and serve conditioning *identical* rather than
merely close. Block size is not a lever: re-evaluating the same checkpoint at
block 32 and 48 moved cards acc@48 by only +0.5 and +0.8, because acceptance is
limited by where the draft first errs, not by how many slots it is offered.

**3. What data to add, in order of expected value.**
- **Fix the duplicate-query jobs.** 177 of 1496 harvest jobs were byte-identical
  queries (the `DASHBOARDS` templates that ignore `{c}` are enqueued once per
  city). They cost 12% of the corpus and 30% of the compose family. Give those
  templates real per-city variation.
- **Longer cards.** 13% of card generations exceed 2048 tokens, which is exactly
  where the user query falls out of reach of draft layers 0-3. That regime is
  under-represented and is where acceptance should degrade most.
- **More composition breadth**, especially 3+ domain cards and domains that
  never co-occur in the current matrix — unseen-combos is the slice with the
  most remaining headroom (39.91 against a 42.55 copy ceiling).
- **Keep the general data.** It is 27% of the corpus and prevented forgetting:
  general acc@48 went *up* (18.71 -> 27.50), and the incumbent only manages 7.71
  there, so DFLASH also fixes a weakness NGRAM has on non-card traffic.

**4. Independent of any of this: let the NGRAM trie index the request.**
The deployed suffix automaton indexes the external corpus and its own output but
not the 53k prompt. Indexing it moves cards 38.00 -> 41.02 and unseen combos
31.08 -> 38.57 in offline replay. That is most of the acc@48 gain of the trained
draft, for a configuration change, with no model, no GPU, and no train/serve
risk. If DFLASH cutover slips, this is the fallback — and it is worth measuring
end-to-end regardless.

## 7. Caveats, stated plainly

- **The NGRAM offline numbers are a proxy.** `src/copy_analysis.py` replays a
  longest-suffix-match drafter at block 16 so it is directly comparable to the
  DFLASH simulator. Production NGRAM runs at block 32 with a real suffix
  automaton and a SAM budget, so its true verify count differs. The offline
  numbers are for *mechanism* attribution; the §5 end-to-end benchmark is the
  authoritative speed comparison, and the two agree in direction on every slice.
- **Parity is 97.2%, not 100%.** The residual is bf16 near-ties (§3), so the
  deployed model's acceptance will differ slightly from the simulator's. The
  end-to-end benchmark does not depend on the simulator and confirms the gain.
- **Benchmark scale is 17 queries, single stream.** Enough to establish a ~1.8x
  effect that is consistent across every slice and both mean and median, not
  enough to quote a precise figure. Multi-concurrency is unmeasured.
- **Held-out means held-out sequences**, and unseen-combos additionally means
  (template, city) pairs never trained on, with both the template and the city
  seen separately. It does **not** mean a new prompt: all card traffic shares one
  172KB system+user prompt, so these numbers describe this deployment, not
  transfer to a different app catalogue.
- **The 8-epoch run is not tuned.** One epoch was the brief; 8 epochs was 19
  minutes and clearly better, and the loss was still falling. Neither learning
  rate, anchor sampling, nor epoch count was searched.
- **Two 27B servers do not fit on this card** with prefill headroom. The
  benchmark arms ran sequentially. During the swap, production was down for
  ~12 minutes; see `PROGRESS.md` for the full outage record.
