# Context-window / latency notes — one-turn card generation

What was changed, what it bought, and the numbers that justify each piece.
All measured Aug 2026 on the GH200 boxes against real phone traffic.

## 1. No-router single turn

The AMA routing round-trip is gone (`app/app/src/main.rs`, commit 7cb331f).
`l0_prompt_all` inlines all 11 L0 app specs + exemplars (~172KB / ~43k tokens,
byte-stable) plus the picking rules; the model self-selects the app and writes
the card in ONE generation. Verified across 5 apps. The kernel's per-session
cwd hint is suppressed via `OCTOS_OMIT_WORKSPACE_HINT` (octos branch
stable-prefix-cache) because it broke prefix stability.

## 2. Byte-stable prefix discipline

Prompt segments ordered by mutation frequency (stable catalog first, per-query
last), so the KV cache prefix survives across requests. Wiretap-verified 99.99%
shared prefix; GLM/Z.ai reported cached_tokens 65,792/65,825 (99.95%).

Caveat that cost a day: **cache saves latency only on slow-prefill stacks.**
GLM and the GH200 FP8 builds prefill fast enough that caching is a cost win,
not a latency win. Do not promise latency from cache alone.

## 3. The overlap-scheduling discovery (the big one)

`--disable-overlap-schedule` + `mamba-radix-cache-strategy no_buffer` were
DFlash-era legacy on both boxes. Switching to `extra_buffer` (ping-pong
donation path) permits overlap scheduling:

| model | before | after |
|---|---|---|
| Qwen3.8-27B FP8 | ~350 tok/s | 799-883 tok/s |
| C2Rust FP8 | ~700 tok/s | 988 tok/s |

Requires `mem-fraction-static <= 0.75` (GDN l2norm OOMs at 0.85). The overlap
gate in sglang is a strategy-conditional assert (server_args.py:4826).
Launcher: `launch_qwen_ab.sh` on the Qwen box (defaults encode all of this).

## 4. Where the remaining latency lives

With a warmed cache, phone-observed generation is decode-bound plus render:
realize/makepad settle on device adds seconds; poster fetches settle ~18s worst
case. The draft-model workstream (../draft-training/) attacks decode; the
render side is unaddressed.
