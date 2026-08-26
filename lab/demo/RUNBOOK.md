# octos-one customer demo — runbook

## What was broken when we started (fixed)

Generation was **completely dead**. Every prompt failed after ~61s with
`failed to send streaming request to OpenAI`. Cause: the phone was last
provisioned on 2026-08-18 with `prov_gh200`, pointing at
`http://192.222.58.176:30878/v1` — the GH200 that was later shut down. The app
was faithfully calling a machine that no longer exists.

Re-provisioning to `prov_glm53` (Z.ai GLM-5.3) restored it. Verified: cards
generate, render, and score 6/10 on the app's own UX critic.

## The thing that would have wrecked the demo

On Android, when `MAKEPAD_DEV_GOAL_FILE` is unset the app falls through to a
mission **baked into the APK** (`dev_goal_movie.txt`) and self-starts a
card-rewriting loop. Measured live: it regenerated and replaced the on-screen
card every ~60–90s, cycling round 1→2→3→4. Mid-demo it would have overwritten
your card, in front of the customer, with an unrelated box-office card.

There is no off switch by design. The fix exploits the read path — an
unreadable file yields `None` and the loop never starts:

    --es makepad.DEV_GOAL_FILE /data/local/tmp/__no_dev_loop__

`preflight.sh` does this and then asserts `0 devgoal lines`.

## Launch

    cd ~/home/octos-one/lab/demo
    ./preflight.sh glm        # or: ./preflight.sh gh200

Green across the board = safe to present. Any red line = do not start.

## Latency — measured on this phone, this prompt

| route | first call (cold) | subsequent (warm) |
|---|---|---|
| GLM-5.3 via Z.ai | **~174s** to card | ~56–76s |
| GH200 C2Rust FP8 | ~4–5s | ~4–5s |

Cold-start is the ~42k-token octos prompt being prefilled. **Always burn one
throwaway prompt before the customer is watching** — it moves the demo from a
3-minute wait to a 1-minute one.

On GLM, narrate while it generates: the streaming card DSL is itself
interesting, and the status page (below) shows real progress.

**If the GH200 is available, use it.** 4–5s versus ~60s is the difference
between a product and a demo of a product.

## Live status while presenting

    adb forward tcp:8686 tcp:8686
    open http://localhost:8686/

Shows phase, streamed bytes, card size and the UX score. Useful as a
second-screen "it is really working" signal.

## Prompt territory

Measured: on a deliberately hard corpus of 10 design schools, generated cards
average 2.5/10 against an HTML control at 7.25 — the renderer lacks stroke,
texture, serif display and hard shadow. But that is the *hard* corpus. In the
territory the renderer serves well the same engine produces genuinely premium
output (see `demo_state.png`: photo backdrop, glass panels, live satellite).

**Safe — rehearse from these:**
- `weather in <city>` — the strongest card. Photo backdrop, live satellite.
- `glass weather in <city>` / `dark weather in <city>` — style modifiers work.
- `<ticker>` or `top gainers` — stock.
- `earthquakes` — USGS live feed.
- `directions to <place>` — nav, opens on the actual route.

**Avoid on stage:** anything asking for heavy graphic style (art-deco frames,
textures, outlined/brutalist looks). Those are the measured weak spots.

## Fallback

If live generation fails, the app retains the last good card, and
`lab/style-factory/baselines/` holds vetted renders. Recovering is a relaunch
via `preflight.sh` — roughly 15s plus one warm-up.

## Known cosmetic issues (do not fix before the demo)

- Panel borders emit but never paint (widget-layer bug, unresolved).
- `_palette_dark.splash` read `l0_hairline` 27 lines before its declaration,
  logging a scope error on every render — fixed here, but the fix is compiled
  into the APK via `include_str!`, so it needs a rebuild to take effect on the
  phone. Harmless to leave: borders do not paint anyway.

---

# H100 deployment (2026-08-27)

The GH200 is gone. Qwen3.8-27B now runs on the Nebius H100 80GB.

    ssh -i ~/home/tensordock/ottos-one.pem octos-one@89.169.125.111

Username is `octos-one` — it matches the key name, not the instance name; Nebius
takes it from cloud-init at creation and has no fixed default.

## What is deployed

- `Qwen/Qwen3.8-27B-FP8` (30.9 GB) at `~/models/target`. FP8 runs natively:
  the H100 is compute capability 9.0. Restart with `~/serve.sh`.
- NGRAM speculation over `~/corpus/cards.jsonl`. **Measured: accept length
  27.95 of 32 draft tokens, accept rate 0.869.** Card DSL is templated enough
  that n-gram copying nearly saturates the draft window.
- Stock `lmsysorg/sglang:latest` (0.5.18). No OminiX fork needed — DFLASH is
  upstream now. See the DFlash2 note below.

## Two flags that decide whether this boots in 80s or 90 minutes

    --env SGL_ENABLE_JIT_DEEPGEMM=0     # <- the important one
    --disable-prefill-cuda-graph

Without the first, SGLang JIT-compiles FP8 DeepGEMM kernels at every boot,
running a 65536-step warmup per CUDA-graph bucket. Measured: still going after
30 minutes, ~11000 warmup log lines. With it: **server ready in 80 seconds.**
Mounting a JIT cache dir did not help — the cache stayed empty at 0 files.

## Latency

`max_total_num_tokens=664937`, context 131072. Raw generation is 37.7 tok/s
(400 tokens in 10.6s). End to end a card takes **~60s**, dominated by prefill of
the ~42k-token octos prompt, not decode — decode is fast because speculation is
accepting ~28 tokens a step.

## The network shape, which is not optional

**The phone cannot reach 89.169.125.111.** Measured 100% packet loss from the
device while the Mac reaches it fine — the GFW blocks the Nebius range. So:

    phone -> adb reverse 30878 -> Mac -> ssh -L 30878 -> H100

and the phone profile (`prov_h100t`) points at `http://127.0.0.1:30878/v1`.
`demo.sh` rebuilds the tunnel and the reverse every run. **The phone must stay
USB-tethered to this Mac.**

## Known blocker at handover: no phone internet

Live data (weather, stock, quake, satellite) is fetched **by the phone,
directly** — the tunnel carries only the LLM. The phone's WiFi dropped
mid-session (`NETWORK_DISCONNECTION_EVENT` at 03:14:08) and did not recover:
it cannot reach 8.8.8.8, open-meteo, or even baidu.com, so this is a dead link
rather than the GFW.

Consequence: **cards generate and render correctly but show `—°` everywhere.**
Verified end to end — the card DSL is accepted, the layout is right, the data
is empty.

Routing the phone's HTTP through the Mac (`settings put global http_proxy
127.0.0.1:7897` + `adb reverse 7897`) does reach the internet — data-fetch
errors went to zero — but HTTPS then fails with `SSLPeerUnverifiedException`
through Clash, and excluding localhost to save the LLM path brings the data
errors back. That setting has been cleared; it is not a working answer.

**Fix the phone's WiFi and everything works.** That is the only outstanding item.

## DFlash2

`z-lab/Qwen3.8-27B-DFlash2` is downloaded to `~/models/draft2` (3.6 GB) and
`~/serve_dflash2.sh` is written and ready. It is **not running**, because stock
sglang 0.5.18 ships DFlash v1 only (`models/dflash.py`) and has no
`selector_rank` / `conv_group_size` — the v2 draft declares
`DFlash2DraftModel`, `block_size: 8`, and target layers `[5,19,33,47,61]`.
Running it needs the OminiX fork built from source.

Worth knowing before spending that time: the GH200's fast production endpoint
was **never running DFlash**. Its launcher says
`--speculative-algorithm NGRAM`; only the served-model *name* said DFlash. And
NGRAM is currently measuring 0.869 acceptance on this workload, against the
DFlash2 card's own published 20.14%. NGRAM is very likely the better choice
here, not a fallback.
