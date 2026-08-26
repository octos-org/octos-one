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
