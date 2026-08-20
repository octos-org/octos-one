# CONDITIONING.md — exactly what the DFLASH draft sees at serve time

Source of truth read for this document (all line numbers verified 2026-08-19):

| short name | path |
|---|---|
| `worker` | `/home/ubuntu/qwen38-h200/phase0/dflash_worker_v2.py` (1820 lines) |
| `utils`  | `/home/ubuntu/qwen38-h200/phase0/dflash_utils.py` |
| `model`  | `/home/ubuntu/qwen38-h200/phase0/sgl/dflash_model.py` (copied out of `qwen-ab`:`/sgl-workspace/sglang/python/sglang/srt/models/dflash.py`) |
| `qwen35` | `qwen-ab`:`/sgl-workspace/sglang/python/sglang/srt/models/qwen3_5.py` |
| `lproc`  | `qwen-ab`:`/sgl-workspace/sglang/python/sglang/srt/layers/logits_processor.py` |
| `dkern`  | `qwen-ab`:`/sgl-workspace/sglang/python/sglang/kernels/ops/speculative/dflash.py` |
| `sargs`  | `qwen-ab`:`/sgl-workspace/sglang/python/sglang/srt/server_args.py` |

**TL;DR — the one sentence that gates the trainer.** The draft model never
embeds the context tokens. Its KV cache for every context position is
*materialized from the target model's hidden states* at 5 tapped target layers;
only the `block_size` "block" positions run through the draft's own Q path, and
their input embeddings come from the **target's** embedding table applied to
`[bonus_token, MASK, MASK, ... ]`. Training must reproduce that or it trains a
different model than the one that will be served.

---

## 1. The two-part input

### 1.1 Context positions — target hidden states, not tokens

For every already-committed token position `p` in `[0, seq_len)` the draft KV
cache holds K/V derived from the **target's** hidden state at `p`:

```
target_hidden[p]  = concat over L in target_layer_ids of  resid_stream_in(L)[p]   # [25600]
ctx_hidden[p]     = hidden_norm( fc( target_hidden[p] ) )                          # [5120]
for each draft layer l in 0..4:
    k, v = layers[l].self_attn.kv_proj_only(ctx_hidden[p])   # Q is never computed
    k    = layers[l].self_attn.apply_k_norm(k)               # per-head RMSNorm
    k    = layers[l].self_attn.apply_k_rope(position=p, k)   # GLOBAL position p
    draft_kv_pool[l][slot(p)] = (k, v)
```

- projection: `model:382-394` (`project_target_hidden` = `hidden_norm(fc(x))`)
- per-layer KV materialization: `worker:1119-1137` (prefix-valid path) and
  `worker:1165-1192` (sequential path); a fused Triton path
  (`worker:1193`, `_append_target_hidden_fused`) is the default on CUDA and is
  numerically the same thing.
- `prepare_context_hidden_for_kv` is the **identity** for the plain
  `DFlashDraftModel` (`model:377-380`) — i.e. all five draft layers consume the
  *same* `ctx_hidden`, each with its own `kv_proj`. (Only the Laguna subclass
  overrides it, `model:551`; our checkpoint is `DFlashDraftModel`.)
- `kv_proj_only` slices the fused QKV weight and skips Q entirely (`model:202-225`).

**Consequence for training:** `fc` and `hidden_norm` are *trainable* parameters
that sit in front of the KV cache. You cannot precompute/cache `ctx_hidden`
across optimizer steps; you must cache the raw `[N, 25600]` target features and
re-project every step.

### 1.2 Which target layers, and what exactly is tapped

`config.json` of `Qwen3.8-27B-DFlash-0e6412a`:

```json
"dflash_config": { "mask_token_id": 248070, "target_layer_ids": [1, 16, 31, 46, 61] },
"num_target_layers": 64
```

The target (`Qwen3_5ForConditionalGeneration`, 64 layers, hybrid
linear/full attention, `full_attention_interval: 4`) marks those layers with
`_is_layer_to_capture` (`qwen35:1278-1281`) and each marked layer appends one
tensor while preparing attention (`qwen35:1320-1329`).

What is appended is the **residual stream entering that layer**, i.e. the
output of layer `id-1` *before* `input_layernorm` — see
`communicator.prepare_attn_and_capture_last_layer_outputs`, which appends
`residual` (post `prepare_attn` fold), not the normed `hidden_states`.
So `target_layer_ids=[1,16,31,46,61]` means "outputs of target layers
0, 15, 30, 45, 60".

They are concatenated in list order on the last dim under
`CaptureHiddenMode.FULL` (`lproc:645-647`):
`hidden_states_to_store = torch.cat(aux_hidden_states, dim=-1)` → `[N, 5*5120 = 25600]`,
which is exactly `fc.in_features` (`model:365-368`, asserted at `model:383-393`).

### 1.3 Block positions — the only tokens the draft embeds

Per decode step the worker builds a fixed `[bs, block_size]` block
(`dkern:_prepare_dflash_draft_block_contig_kernel`, mirrored by the eager
fallback at `worker:1514-1516` / `worker:1533-1535`):

```
block_ids[:, 0]  = bonus_token          # last token the TARGET committed
block_ids[:, 1:] = MASK_ID              # 248070
positions[:, j]  = seq_len + j          # j = 0..block_size-1, GLOBAL positions
cache_loc[:, j]  = req_to_token[req, seq_len + j]
```

The embedding is taken from the **target** model, not the draft
(`worker:1548-1553`):

```python
embed_module = target_model.get_input_embeddings()   # worker:1477
noise_embedding = embed_module(block_ids)
input_embeds = noise_embedding.view(-1, hidden)
```

and the draft is invoked with `input_embeds` (it *refuses* to run without them,
`model:405-409`). The forward is a `ForwardBatch` in `ForwardMode.TARGET_VERIFY`
with `capture_hidden_mode=NULL` (`worker:1596-1611`).

So a single draft step is:

```
draft_out[j] = DFlashLayers( embed_target(block_ids)[j], pos = seq_len + j,
                             attending over { materialized ctx KV } ∪ { block KV } )
```

### 1.4 Head: there is none

`DFlashDraftModel` has no `lm_head` and no embedding weights (`model:322-330`).
The final `norm` output is multiplied by the **target's** `lm_head.weight`
(`_DflashDraftSampler.__call__`, `worker:118-130`; eager path
`worker:1626-1631`). Draft tokens are a plain greedy argmax over the target's
original vocab (`num_org` rows).

Critically, position 0 of the block is dropped:

```python
hs = hidden_states.view(bs, block_size, -1)[:, 1:, :]     # worker:120-122
```

so a block of size `B` proposes `B-1` tokens.

---

## 2. The MASK token id — CLAUDE.md is wrong here

CLAUDE.md says "mask token `<|MASK|>` id 248077". Verified against the target
tokenizer:

- there is **no** `<|MASK|>` string in `vocab.json` or `added_tokens_decoder`;
- id `248077` is not an added token at all (added tokens stop at 248076);
- id **248070** is `<|audio_start|>`, and that is what `dflash_config.mask_token_id`
  says.

`utils:490-492` defaults the *string* to `DEFAULT_DFLASH_MASK_TOKEN = "<|MASK|>"`
but `_resolve_mask_token_id` (`worker:714-795`) short-circuits on the explicit
`mask_token_id`: it only cross-checks the tokenizer if the string happens to be
in the vocab, which `<|MASK|>` is not. So the served MASK is **248070**, an
audio special token repurposed as an unused slot in a language-only deployment.

> **Trainer must use `MASK_ID = 248070`.** Using 248077 would embed
> `<|audio_pad|>`+1 (an unused/garbage row) and silently destroy conditioning.

---

## 3. Block size 16 vs "serving draft 32"

- The draft checkpoint declares `block_size: 16` (top-level in `config.json`,
  read by `resolve_block_size`, `utils:404-405`).
- The worker's block size is **`server_args.speculative_num_draft_tokens` when
  set**, and the checkpoint value is only used as a fallback
  (`worker:198-215`). A mismatch is a `logger.warning`, not an error.
- The current production launcher runs **NGRAM**, not DFLASH:
  `--speculative-algorithm NGRAM --speculative-num-draft-tokens 32`
  (`launch_qwen_ab.sh`). The "draft 32 / sam 31" in CLAUDE.md is the *n-gram*
  proposal length and the external-SAM budget, and has nothing to do with the
  DFLASH block. There is no DFLASH block size of 32 in effect anywhere today.
- Accepted-token accounting (`dkern:_dflash_accept_bonus_contig_kernel`):
  `candidates[0]` is the bonus token (already verified), and the loop compares
  `candidates[col+1]` vs `target_top1[col]` for `col in 0..B-2`. Therefore
  **`accept_len ∈ [0, B-1]`** and `commit_len = accept_len + 1`.
  With `B=16` the ceiling is **15 accepted drafts + 1 bonus = 16 tokens/verify**.

  That is the ceiling the Goal-5 gates ("cards ≥ 40/48") must be read against:
  48 is a *48-token window* in the accept-length simulator, i.e. at least three
  consecutive B=16 blocks, not one block.

  The exact identity (proved by `src/test_accounting.py`) is

      accepted-in-a-K-window  =  K  -  number of verifies used

  because every verify commits `accept_len + 1` tokens and that `+1` is the
  target's own token, never an accepted draft. So the gates translate into a
  required mean accept length as:

  | gate | verifies allowed for 48 tokens | tokens/verify | mean accept_len (B=16) |
  |---|---|---|---|
  | cards >= 40/48         | <= 8  | >= 6.0  | >= 5.0  |
  | unseen-combos >= 25/48 | <= 23 | >= 2.1  | >= 1.1  |
  | general >= 8/48        | <= 40 | >= 1.2  | >= 0.2  |

  and the ceiling at B=16 is 48 - ceil(48/16) = **45/48**. Read that way the
  gates are demanding on cards and lenient elsewhere, which matches their intent
  as a regression guard plus a "must not be useless on novel compositions" bar.

**Decision for training:** train at `B=16` (the checkpoint's own block size), and
build training windows at K ∈ {8,16,32,48} as CLAUDE.md asks, but note that
K>16 only makes sense as *multi-block* windows (the simulator chains blocks);
a single forward with B=32 would be running the model at a block length it was
never trained on. Both are measured in `eval_accept.py`.

---

## 4. Positions, sliding window, and the draft window

- Draft layer types (`config.json`): `["sliding_attention"×4, "full_attention"]`
  with `sliding_window: 2048` (`model:46-69`). So **draft layers 0-3 see only the
  last 2048 keys; only layer 4 sees the whole context.**
- RoPE positions are **global** everywhere: context KV is roped with the token's
  true position (`worker:1131`, `_append_target_hidden_*`), and block positions
  are `seq_len + j` (`dkern`, `worker:1517-1521`).
- `--speculative-draft-window-size W` (`sargs:1811-1813`) is the *only* knob that
  truncates what the draft sees. When set, `use_compact_draft_cache=True`
  (`worker:174-177`) and per step:
  - `draft_prefix_len = min(seq_len, W)` page-aligned up (`worker:607-623`);
  - `_rebuild_compact_draft_cache` (`worker:662-712`) rewrites the draft's
    `req_to_token` row to the **last W committed slots** plus the block slots.
  - Positions are *not* rebased — the same global positions are used. So W is a
    pure visibility window on the most recent W tokens.
  - It is **not set** in the current launcher (default `None` = full context).

### Why this matters for this project specifically

In `warm_request.json` the user request is the **last 17 characters of a 172,508
character user message** (`...===== END REFERENCE =====\n\nUser request: weather in Berlin`).
The prompt is 53,127 tokens. So the query — the source of every "novel" value the
NGRAM trie cannot draft (city name, tickers, place names) — sits at the very end
of the context, immediately before the first generated token.

Consequences:

1. A copy-capable draft only needs a *short recent window* to do the copying,
   because the query is adjacent to the generation. This is what makes training
   tractable at all: we do **not** need 53k tokens of target hidden states per
   example.
2. But 4 of 5 draft layers already have a hard 2048 window, and generations run
   to ~1.7k–4k tokens (see `harvest/STATS.md`). Once the card is longer than
   ~2048 tokens, the query has scrolled out of those layers' reach and only the
   single full-attention layer 4 can still reach it — and if `W` is ever set
   below the card length, nothing can.
3. **Training decision (D2):** train with a bounded context window
   `W_train = 4096` tokens of target hidden states ending at the draft position.
   This (a) strictly contains the 2048 sliding window of layers 0-3 so those are
   *exact*, (b) leaves layer 4 with a 4096-token view instead of 53k, which is
   an approximation we accept and record, and (c) covers query+card for the
   overwhelming majority of harvested generations.
   The matching serve-time flag is `--speculative-draft-window-size 4096`, which
   makes train and serve conditioning **identical** rather than merely close.
   This is the recommended serving configuration and is called out in FINDINGS.md.

---

## 5. Lifecycle: where hidden states come from at each stage

**Prefill** (`worker:1373-1428`): the target runs with
`capture_hidden_mode=CaptureHiddenMode.FULL`, and *all* prompt-token aux hidden
states are immediately written into the draft KV cache
(`worker:1416-1425`); `logits_output.hidden_states` is then dropped.
`bonus_tokens` for the first draft step = the target's first sampled token.

**Each decode step** (`worker:1466-1810`):
1. build the block (§1.3), draft forward, argmax vs the target `lm_head` → `B-1` drafts;
2. `candidates = [bonus] + drafts`, run the **target** over all `B` candidate
   positions in `TARGET_VERIFY` with `CaptureHiddenMode.FULL` (`worker:1640-1660`);
3. accept the longest matching prefix (`dkern`), `commit_len = accept_len + 1`;
4. **the verify-step target hidden states for the committed prefix are appended
   to the draft KV** (`worker:1785-1800`, prefix-valid write bounded by
   `commit_lens`). Rejected positions are never committed.

So at every point the draft's context KV is *exactly* the target's hidden states
for the tokens actually in the sequence — teacher forcing at serve time. This is
the single most important property for the trainer: **teacher-forced training on
recorded temp-0 target output is not an approximation of serving, it is
literally the serving conditioning**, provided the hidden states come from the
same 5 taps of the same target model.

---

## 6. The trainer contract (what `train_dflash.py` must do)

For a recorded sequence with token ids `t[0..N)` (prompt ⧺ completion) and target
features `H[p] ∈ R^25600` for `p` in the training window:

```
pick a draft anchor a  (an index inside the completion)
ctx positions   : p ∈ [a - W_train, a)          # committed tokens, KV from H[p]
block positions : j = 0..B-1 at global position a + j
block input ids : [ t[a] , MASK, MASK, ... ]     # t[a] is the bonus token
                  embedded with the TARGET embedding table
labels          : out[j] must equal t[a + j]  for j = 1..B-1
                  (out[0] is discarded, exactly as the sampler does)
loss            : cross-entropy of (draft_hidden[j] @ target_lm_head.T) vs t[a+j],
                  j = 1..B-1, over the target's original vocab rows only
```

Non-negotiables, each of which is a silent-failure mode if broken:

1. `MASK_ID = 248070`.
2. Block position 0 holds the real token `t[a]`, **not** a mask, and its output is
   dropped from the loss.
3. Context KV comes from `hidden_norm(fc(H))` **through the trainable fc**, with
   `k_norm` then RoPE at the token's **global** position — not the window-local one.
4. Block embeddings come from the **target's** `embed_tokens`, frozen.
5. Logits come from the **target's** `lm_head`, frozen, sliced to the original
   vocab (`num_org`), matching `_DflashDraftSampler` / `_greedy_sample_from_vocab_parallel_head`.
6. Draft layers 0-3 must use a 2048 sliding-window mask; layer 4 full within the
   window.
7. The aux features must be the residual-stream inputs of target layers
   1/16/31/46/61 concatenated **in that order**.

