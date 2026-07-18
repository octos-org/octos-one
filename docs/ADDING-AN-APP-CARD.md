# Adding a new app card

This walks through adding a whole new app type end-to-end, using **crypto** as a
worked example (a live coin price card). You do NOT copy an existing card — every
app is **assembled** by the model from a requirements spec + the shared widget
patterns. Read `apps/stock/app.md` for the house style before you start.

There are (at most) two layers to touch:

- **Data** — only if your app needs a *new* live source → the framework fork (rebuild).
- **App definition** — always → `a2app/` (no rebuild; often no `app/` code at all).

> **Do NOT write the card yourself, and do NOT add an exemplar.** The whole design
> is that the on-device model generates the card from requirements. A hand-authored
> `.splash` template makes the model a verbatim copier and defeats composition.

---

## Step 0 — pick a live data source

Use a **keyless, CORS-free JSON** API (the card fetches it directly at render
time, through the device's network/proxy). Examples in use: open-meteo (weather),
Yahoo Finance chart (stock), Hacker News (news), OpenStreetMap Overpass (places).
For crypto:
`https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd`.

Verify it returns clean JSON and works **without** an API key before continuing.

---

## Step 1 — add a `sys.<domain>` data helper (framework fork, only for a new source)

If an existing helper already covers your data, skip this. Otherwise, in the
framework fork `widgets/src/splash.rs`, add a helper next to `sys.stock` /
`sys.news`. It should:

1. Read its args (`script_value!`, `cast_to_string`), sanitizing anything that
   goes into a URL path (see `sanitize_ticker`).
2. Build the API URL.
3. Fetch + pluck:
   ```rust
   let out = match vm.host.cx_mut().script_data_fetch(&url) {
       Some(bytes) => json_pluck(&bytes, &path).unwrap_or_else(|| "—".to_string()),
       None => "—".to_string(),
   };
   vm.bx.heap.new_string_from_str(&out)
   ```
   `script_data_fetch` (dedup + async + bounded retries), `json_pluck` (dot-path,
   array indices, ISO→HH:MM), and the `DATA_FETCH_EPOCH` re-eval are all reused for
   free — see [ARCHITECTURE.md §5](ARCHITECTURE.md#5-the-splash-card--live-data-binding).
   Add a numeric variant (returning an `f64`, `-9999` while loading) if cards need
   to *branch* on the value (like `sys.weathernum`).

**Add your helper name to `body_binds_live_data()`** (same file) so cards that
call it arm the frame pump and re-evaluate when the fetch lands:
```rust
fn body_binds_live_data(body: &str) -> bool {
    body.contains("sys.weather") || body.contains("sys.airquality")
        || body.contains("sys.aqinum") || body.contains("sys.stock")
        || body.contains("sys.news") || body.contains("sys.movers")
        || body.contains("sys.places")
        || body.contains("sys.crypto")           // ← add
}
```

Rebuild is required for helper changes (native code) — see
[BUILDING-ANDROID.md](BUILDING-ANDROID.md). Document the helper's keys in
`a2app/widgets/sys-helpers.md`.

---

## Step 2 — write the app definition (`a2app/`, no rebuild)

Create a **requirements spec** and its **lint rules** — no exemplar:

```
a2app/apps/crypto/app.md        # what the card must contain (requirements + failures)
a2app/apps/crypto/lint.json     # machine-checkable rules the validator enforces
```

**`app.md`** — a requirements spec, not a card. Model it on `apps/stock/app.md`:
- State up front: "YOU generate this card by ASSEMBLING the widget patterns —
  there is no exemplar." Point at the pieces it should use (`widgets/design-system.md`
  for the look, `widgets/interaction.md` for tap/state/nav, `widgets/sys-helpers.md`
  for data).
- The FIRST line of the block is `// name: crypto-app`.
- List the **mandatory sections** and, for each value, the exact `sys.crypto(...)`
  binding — stress that **every number is a helper call, never hardcoded**.
- Reference the design system for colors/type/spacing instead of restating them,
  and reuse a parent app's **named blocks** if you're extending one (e.g. a chart).
- End with a `## Failure conditions` section (the human-readable contract; the
  lint file is the executable half).

**`lint.json`** — plain `(pattern, min-count)` substring rules mirroring the
failure conditions. Copy the shape of `apps/stock/lint.json`:
```json
{ "rules": [
  {"desc": "the first line must be `// name: crypto-app`", "pattern": "// name: crypto-app", "min": 1},
  {"desc": "every displayed value binds sys.crypto", "pattern": "sys.crypto(", "min": 4}
] }
```
Set the min counts from your mandatory structure (a compliant card's real count),
not aspirationally — an over-strict rule forces needless repair turns; an
under-strict one lets a broken card pass. Substring lint can't express OR /
forbid / distinct-count; leave those to the prose failure conditions.

---

## Step 3 — make the AMA route to it (`a2app/framework.md`, no rebuild)

The AMA reads its routing list from the **injected** `framework.md` — so a new
app is a one-line content edit, no `app/` code:

- Add `crypto` to the app-type routing list with a one-line domain description
  (`crypto — a cryptocurrency's price (BTC, ETH, 比特币)`).

That's usually all. The client's **create-if-missing** path (`compose_app` when
the routed id has no boot agent but `apps/<id>/app.md` exists) spins up a peer
agent for `crypto` on demand — no `clear_chat` edit, no `AMA_SYSTEM_PROMPT` list
edit, no `APP_SPLASH_ROUTER` edit (that prompt defers to the routed id).

**Optional — a boot-time agent.** If you want the app's session created at
startup (e.g. to warm it), add it in `clear_chat`:
```rust
let crypto = agent.create_session(cx, app_cfg());
self.apps.push(AppRecord::with_domain(crypto, "Crypto", "crypto"));
```
This is only an optimization; routing works without it.

---

## Step 4 — deploy & verify

1. **No build step for content.** octos discovers `apps/<id>/app.md` +
   `lint.json` automatically when it assembles the `app-cards/` tree at inject
   time — just create the files under `a2app/` (there is no `FILES` list). Keep
   the tree under the kernel's `memory.max_inject_tokens` (the app auto-sets 40000
   at boot; `wc -c a2app -r` / 4 estimates the tree), or octos truncates the tail
   app — see the ⚠️ in [ARCHITECTURE §3](ARCHITECTURE.md#3-the-a2app-memory-how-an-agent-knows-an-app).
2. Deploy the `a2app/` tree to the device (as `app-cards/`) — and rebuild the APK
   **only if** you added a helper in Step 1. See
   [BUILDING-ANDROID.md](BUILDING-ANDROID.md#deploy-the-app-card-memory).
3. Test: send a crypto intent; confirm the log shows the AMA routing/composing to
   `crypto`, the card renders, the validator passes (or repairs once), and
   `grep -c 'sys.crypto' <saved card>` shows the values are DSL-bound (not
   hardcoded), matching the live API.

---

## Checklist

- [ ] Keyless JSON source verified
- [ ] `sys.<domain>` helper (+ numeric variant if branching) and
      `body_binds_live_data` updated (framework fork) — **only if new source**
- [ ] Keys documented in `a2app/widgets/sys-helpers.md`
- [ ] `apps/<id>/app.md` — a **requirements spec** (no exemplar), `// name:` first
      line, mandatory sections, failure conditions
- [ ] `apps/<id>/lint.json` — executable rules with realistic min counts
- [ ] `a2app/framework.md` routing list updated (one line)
- [ ] `a2app/` tree size fits `max_inject_tokens`
- [ ] Deployed; rebuilt **only if** a helper was added; verified live on device
      (routes, renders, validates, values match the API)
