# lab/ — inference & card-quality workstreams

Working notes and code for the performance/quality push around on-device card
generation. Three workstreams, each with its own folder. Live experiment data
stays on the machines that produce it (Qwen GH200 box, Mac scratchpad); what is
versioned here is everything needed to understand, reproduce, or restart the
work.

## prefix-cache/ — context-window & latency work

Getting one-turn card generation fast: byte-stable prompt prefixes for KV-cache
reuse, the no-router single-turn architecture (`l0_prompt_all` in
`app/app/src/main.rs`), and the overlap-scheduling discovery that took
Qwen3.8-27B from ~350 to ~800-880 tok/s on the GH200. See NOTES.md for the
numbers and the flags that matter.

## draft-training/ — compositional DFlash draft for Qwen3.8-27B

A speculative-decoding draft model specialized to this app's card traffic,
trained overnight by an autonomous agent on the serving box (charter:
CLAUDE.md; conclusions: FINDINGS.md). Headline: all acceptance gates passed,
**1.79x end-to-end decode, 2.03x on novel compositions**, outputs byte-identical
by construction. `harvest.py` generated the 1,496-query distillation corpus on
the production server; `src/` is the full pipeline (hidden-state extraction,
torch draft reimplementation, trainer, accept simulator, serve benchmark).
`TASK-stylist.md` is the follow-on experiment: can a draft trained only on
*beautiful* cards steer the base model under lenient verification.

## beauty-pipeline/ — automated 美学 dataset (render + vision-judge)

Closes the loop the stylist experiment needs: every harvested card is rendered
with live data on a real device (OnePlus 6T, `SEED_L0_FILE` seed hook,
`render_loop.py`), screenshotted, and scored 1-10 against a fixed design rubric
by Claude Opus (`score_opus.py`, headless `claude -p`, vision via Read). The
scored corpus selects the top-quartile "beautiful cards" training set — no human
judge in the loop. `extract_cards.py` dedupes the harvest into unique card
sources. Render-side accommodations (unused-source strip, query-derived state
ledger, VPN tunneling via `adb reverse`) are documented in the scripts
themselves.

Next stage prototyped: gpt-image-2 design mockups translated to L0 DSL by Opus
(design-to-code) to inject *new* graphic language as prompt exemplars — raising
the generator's ceiling instead of only selecting within it.
