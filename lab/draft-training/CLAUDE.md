# Project: Compositional DFlash draft for Qwen3.8-27B

You are working ON the GH200 box (192.222.59.228) that also SERVES production.
Mission: train a speculative draft model that keeps Qwen3.8-27B fast on
COMPOSED app cards (multi-domain pages: weather+events+nav+hotels...) where the
current NGRAM cache structurally fails (it replays fragments; it cannot draft
novel compositions, section seams, or cross-section bindings).

## The machine (do not fight it)

- ONE NVIDIA GH200 (96GB HBM + 480GB unified via NVLink-C2C). Production serving
  container `qwen-ab` (docker, restart=unless-stopped) owns the GPU most of the
  time — port 30878, OpenAI-compatible, model id `Qwen3.8-27B-FP8-DFlash`,
  NGRAM speculation (draft 32/sam 31) + corpus + overlap scheduling +
  extra_buffer mamba radix + `enable_thinking:false`. Launcher (source of
  truth): `/home/ubuntu/qwen38-h200/launch_qwen_ab.sh` (env knobs ABMRR/ABDRAFT/
  ABMEM/ABOVERLAP/ABSTRAT). Known: ABMEM must be <=0.75 with extra_buffer or
  GDN kernels OOM during CUDA-graph capture.
- HARD RULES: never leave the box without a serving container running; never
  `docker rm` qwen-ab except immediately relaunching via the launcher; training
  runs only AFTER the harvest completes and must leave >=30GB HBM free or run
  with the server stopped ONLY if you restart it right after (prefer: train
  small/gradient-checkpointed alongside serving; measure free HBM first).
- tmux session `harvest` is running `/home/ubuntu/qwen38-h200/harvest.py`
  (1,496 temp-0 generations -> `/home/ubuntu/qwen38-h200/harvest/out.jsonl`,
  resume-safe: rerun the script if it died; it health-restarts qwen-ab itself).

## Key artifacts

- `/home/ubuntu/qwen38-h200/warm_request.json` — the REAL production request
  (172KB all-apps prompt). All harvest generations derive from it.
- `/home/ubuntu/qwen38-h200/phase0/` — the DFlash runtime sources copied from
  the serving image: `dflash_worker_v2.py` (79KB — THE conditioning truth),
  `dflash_utils.py`, `dflash_info*.py`. The full sglang tree is inside the
  container at /sgl-workspace/sglang (read via `sudo docker exec qwen-ab ...`).
- Models: target `/home/ubuntu/qwen38-h200/models/Qwen3.8-27B-FP8-017b9c7`
  (block-FP8), existing general draft
  `/home/ubuntu/qwen38-h200/models/Qwen3.8-27B-DFlash-0e6412a`
  (DFlashDraftModel: 5 layers, hidden 5120, vocab 248320, block_size 16,
  mask token `<|MASK|>` id 248077, BF16 single safetensors). INIT FROM THIS.
- NGRAM corpus: `/home/ubuntu/qwen38-h200/ngram-corpus/cards.jsonl`.

## Facts you would otherwise have to rediscover

- Speculation speed = accepted-tokens-per-verify / step-cost. Verify is exact:
  drafts can only affect SPEED, never outputs.
- NGRAM acceptance saturates ~33/48 on cards: templates break at query-derived
  value slots (city names, state initials). L0 cards contain NO live facts
  (no temps/prices — sys.* fills those at render), so nearly all "novel" tokens
  are COPYABLE from the request. A learned draft can learn that copying; a trie
  cannot. That is the entire thesis.
- Current serving: ~800-880 tok/s on corpus-covered cards; ~50 tok/s target-only.
- Train/serve conditioning mismatch is the classic failure: the trainer MUST
  construct draft inputs exactly as `dflash_worker_v2.py` does at serve time
  (context window, hidden-state handoff, mask layout). Read it FIRST.

## Goals (work in order; log everything to PROGRESS.md as you go)

1. WATCH the harvest to completion (`wc -l harvest/out.jsonl` vs 1496; tail
   harvest.log). When done: write `harvest/STATS.md` — counts per family/mode,
   token totals, length histograms, empty/error outputs, and 3 example
   compose-mode cards eyeballed for multi-section structure.
2. STUDY `phase0/dflash_worker_v2.py`: document in `CONDITIONING.md` exactly
   how draft inputs are built (what context the draft sees, how many tokens,
   hidden-state coupling to the target if any, mask token layout, block size
   handling, what "block_size 16" in the draft config means vs serving draft
   tokens 32). Cite line numbers. This gates everything.
3. PROTOTYPE `train_dflash.py`: load the existing draft checkpoint
   (DFlashDraftModel via the container's sglang code or transformers
   trust_remote_code from the model dir), build training examples from
   out.jsonl per CONDITIONING.md ([context][MASK xK] -> next K target tokens,
   K in {8,16,32,48}), cross-entropy on hard labels, BF16, grad checkpointing,
   single GPU. Dry-run on 100 examples: loss decreasing, no OOM, <=30GB HBM.
4. BUILD `eval_accept.py`: the accept-length simulator — replay held-out
   recorded generations, draft each K-window, count leading exact matches.
   Baselines to report: existing 0e6412a draft on cards vs compose vs general.
   (This baseline is valuable BEFORE any training: it tells us how far the
   general draft already gets on compositions.)
5. ONLY IF 1-4 are green and GPU headroom is confirmed: launch a first real
   training run in tmux (`tmux new -s train`), 1 epoch over the pick+general
   data + all compose data, checkpoints to
   /home/ubuntu/qwen38-h200/draft-training/ckpt/. Then eval_accept.py against
   held-out compose set: gates are cards>=40/48, unseen-combos>=25/48,
   general>=8/48, and seam-window (+-10 tokens around section boundaries)
   acceptance reported separately.
6. Write FINDINGS.md: numbers, surprises, and the exact next decisions for the
   human (ship as DFLASH serving? draft window? what data to add).

Do NOT: push to any git remote, change branch protections, touch the phone,
call external APIs, or modify launch_qwen_ab.sh defaults. Everything stays on
this box. If truly blocked, write BLOCKED.md with what you need and stop.
