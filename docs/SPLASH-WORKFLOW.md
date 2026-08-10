# Splash-Workflow: exactly-once effects and capability-scoped tools

*octos-one engineering note — why the new Splash VM's `splash-workflow` layer
matters when LLM-composed cards start doing real things, and why "just call
bash / shell out from a script" is the wrong model on a phone. Documented from a
working session on 2026-07-28.*

A working-session record: what `splash-workflow` guarantees, a **runnable
validation** of that guarantee against a naive script, and the tool-execution
model (capability-scoped, sandboxed) that follows from it — with the octos-one
recommendation.

| | |
|---|---|
| **Date** | 2026-07-28 |
| **octos-one** | branch `nav-decompose`, head `7613544` |
| **ymote/Splash** | `github.com/ymote/Splash`, branch `main`, head `623d57c` |
| **Scope** | `splash-workflow` · `splash-capabilities` · `splash-protocol` · `splash-sandbox` |
| **Companion** | architecture map artifact (DOT ▸ agents ▸ splash-workflow) |

Line numbers below are anchors against ymote/Splash @ `623d57c`. They drift with
edits — treat them as pointers, not contracts.

- [1. Why this matters for octos-one](#1-why-this-matters-for-octos-one)
- [2. Where splash-workflow sits (and what it is not)](#2-where-splash-workflow-sits-and-what-it-is-not)
- [3. The design goal: exactly-once, crash-safe effects](#3-the-design-goal-exactly-once-crash-safe-effects)
- [4. Validation: the crate vs. just a python script call](#4-validation-the-crate-vs-just-a-python-script-call)
- [5. Can it run bash / filesystem tools?](#5-can-it-run-bash--filesystem-tools)
- [6. Ambient bash vs. the capability model — a fit judgment](#6-ambient-bash-vs-the-capability-model--a-fit-judgment)
- [7. Recommendation for octos-one](#7-recommendation-for-octos-one)
- [Provenance](#provenance)

---

## 1. Why this matters for octos-one

octos-one is an **agent OS on a phone**: LLM-composed Splash cards *are* the UI
(see [ARCHITECTURE.md](ARCHITECTURE.md)). Today most cards render and read data.
The moment a card does something **real and irreversible** — publish, pay, send,
write to an external system — a new problem appears that the current runtime does
not solve: **if the phone crashes or the network blips mid-effect, a retry can do
the thing twice.**

`splash-workflow` (a crate in the new [ymote/Splash](https://github.com/ymote/Splash)
VM) is the layer built for exactly that. This note records what it guarantees,
proves it with a runnable test, and draws out the consequence for how cards are
allowed to touch the outside world.

Related: the VM migration itself is covered in
[SPLASH-VS-TAURI.md](SPLASH-VS-TAURI.md) §6 and
[SPLASH-NATIVE-INTEGRATION.md](SPLASH-NATIVE-INTEGRATION.md) §8/§10 ("migrate the
language, re-bind the UI"). This doc is about the *effects/tools* half of that
story.

---

## 2. Where splash-workflow sits (and what it is not)

**It is not a subagent/agent orchestrator.** Its "steps" are steps of one Splash
program, not agents. The unit of work is *a step of DSL calling a
capability-bound tool*, and its job is authority + durability around that call.

The orchestration layer already has an owner: octos's **DOT pipeline**
(`octos-pipeline` — a Graphviz DOT graph of LLM/agent/shell nodes, per-node model
selection, parallel fan-out, filesystem checkpoints, human gates). DOT answers
*"which workers run, in what order?"*. `splash-workflow` answers a different
question: *"is this one effect authorized, and will it happen exactly once even
through a crash?"* They **stack**, they don't compete:

```
DOT (octos-pipeline)     which workers run, in what order, on which model   (macro orchestration)
   └─ Agents             reason, generate, emit a Splash card               (workers)
        └─ splash-workflow   is this effect authorized + exactly-once?      (micro authority/durability)
             └─ Hybrid UI substrate (Makepad self-render + native overlays + AccessKit)
```

For the same reason, DOT — not `splash-workflow` — is the peer of Claude Code's
"dynamic workflow." DOT is a *declarative, durable, server-side* orchestrator;
Claude Code's is an *imperative, ephemeral, in-session* one. Neither is the
exactly-once effect layer.

---

## 3. The design goal: exactly-once, crash-safe effects

The whole `splash-workflow` machine exists to make one guarantee hold: **an
external effect is applied exactly once, even across a crash + retry — and if it
did happen, it can be safely undone.** The lifecycle (traced from
`crates/splash-workflow/src/lib.rs`):

| stage | function (`lib.rs`) | what it does | fail-closed guard |
|---|---|---|---|
| Lease | `approve_with_step_capability_leases` :4811 | one lease per step, narrow-only, bound to the runtime + catalog | `CatalogChanged` |
| Record | `record_derived_operation` :5301 | BLAKE3 key over plan+step+tool+input+nonce; **persist before dispatch** | `InputFingerprintMismatch` |
| Dispatch | `operation_dispatch_request` :5548 | host-sealed frame; the derived key is the worker dedup key | — |
| *(crash)* | — | approval/lease/promise lost; ledger + checkpoint survive | — |
| Reconcile | `ReconcileOperation` :5879 | worker replays state from the dedup key; terminal states monotonic | `InvalidStateTransition` |
| Checkpoint | `checkpoint_after` :5066 | attested completed prefix, ≤16 KiB, anti-rollback storage | `PlanMismatch` |
| Resume | `approve_resume` :6072 | fresh approval, suffix only | `StepPrefixMismatch` |
| Compensate | `approve_compensation` :5653 | one-use, session-bound rollback; only if durably Succeeded | grant-fp / revision drift |

The key architectural fact: **all authority is host-owned and durable.** The
untrusted card never mints its own approval; the "already did this?" answer comes
from a persisted, monotonic ledger, reloaded across the crash.

---

## 4. Validation: the crate vs. just a python script call

This was validated by driving the **real `splash-workflow` crate** through its
public API and comparing against a naive "just call the effect" Python script. The
exactly-once decision is made *by the crate* (the effect fires only when the
crate's reloaded ledger state permits it) — no dedup logic is written on the test
side.

The effect is `release.publish`; applying it twice is the harm.

```
approach                    effect applied   exactly-once?   enforced by
just a python script call        2                NO         nothing   -> VIOLATED
splash-workflow (crate)          1                YES        the crate -> MET
```

The naive script retries after a lost response and double-applies. The
splash-workflow side stays at **1**, decided solely by
`ledger.operation(op_key).state()` read after reloading the ledger from JSON
across the simulated crash. And the crate **actively rejects** every bypass a
naive retry would use:

| attempted bypass | crate response |
|---|---|
| re-record the same operation identity | `REJECTED: DuplicateOperationKey` |
| flip a Succeeded op to Cancelled | `REJECTED: InvalidStateTransition { current: Succeeded, observed: Cancelled }` |
| re-apply Succeeded (idempotent) | accepted as a no-op; **revision unchanged** |
| compensate a still-Pending op | `REJECTED: CompensationRequiresSucceededOperation` |
| compensate a second time | `REJECTED: CompensationAlreadyRecorded` (one-use) |
| reconcile against a tampered plan | `REJECTED: PlanMismatch` |

Two further design goals confirmed the same way:

- **Distinct effects are not over-suppressed.** A genuinely new publish (fresh
  input + nonce) derives a *distinct* key, so dedup lets it through — exactly-once
  keys on *operation identity*, not "was publish ever called."
- **Rollback is real and one-use.** A `cmp-…` compensation key derives *only* for
  a Succeeded op; a second compensation is refused.

The derivation construction itself (domain tags, big-endian length prefixes,
field order, `op-`/`cmp-` prefixes) was independently reimplemented in Python and
matched the crate byte-for-byte, so the identity scheme is exactly as described.

**How the validation was structured** (the gate is the crate, not our code):

```rust
// The effect fires ONLY if splash-workflow's durable state says it isn't done.
fn is_done(ledger: &WorkflowOperationLedger, op_key: &str) -> bool {
    ledger.operation(op_key)
        .map(|op| op.state() == WorkflowOperationState::Succeeded)
        .unwrap_or(false)
}
// ... after a simulated crash + reload-from-JSON:
if !is_done(&reloaded_ledger, &op_key) { world.publish(); } // skipped -> exactly once
```

```python
# the "just a python script call" baseline: no ledger, no state
def publish(): world["published"] += 1
publish()          # dispatch
publish()          # retry after a lost response -> DOUBLE APPLY
```

> The runnable harness (a Cargo binary linking the real crate + a Python
> reporter) is reproducible; it is not committed to this repo. Ask if it should
> live under `octos-one` (e.g. `docs/examples/splash-workflow-validation/`).

**Honest boundary:** the crate guarantees the durable *identity, state machine,
and guards*. A real worker must still consult that state before it performs its
side effect — the crate makes correct behavior enforceable, it does not reach out
and stop a worker that ignores the ledger. That worker-consults-crate step is the
intended contract, and the harness exercises it against the real crate.

---

## 5. Can it run bash / filesystem tools?

Yes — but **never as an ambient shell.** There is no ambient filesystem, process,
or network access. A "run this command" or "read this file" is a **registered
tool** with an explicit name, a call budget, input/output byte bounds, and a
resource grant scoped to one of four kinds — `Executable`, `FileRoot`,
`NetworkOrigin`, `Secret` (`splash-protocol` `ResourceKind`, lib.rs:81). The
script names an **opaque ID**, not a real path or command line; the trusted host
maps it. *"The protocol never treats the identifier as an operating-system path
or command line"* (protocol lib.rs:74-79). So an LLM-composed card cannot choose
`rm -rf /` or read `/etc/passwd` — only the vetted operations the host wired up.

External tools are **deferred-only**: a script must `tool.start("…").await()`; the
host claims the invocation and dispatches it to its own worker/platform adapter
(`docs/external-tools.md`). This is deliberately three layers, and they must not
be conflated:

| layer | crate | responsibility |
|---|---|---|
| grant model | `splash-capabilities` | which tool, how many calls, what resources (deny-by-default) |
| authority + durability | `splash-workflow` | exactly-once, ledger, leases, rollback (§3–4) |
| **OS containment** | `splash-sandbox` | the actual mount policy / executable mediation / egress (Linux bubblewrap + landlock, etc.) |

The docs are blunt that *"external dispatch is a capability boundary, **not** an OS
sandbox"* (`docs/external-tools.md:339`) — real isolation needs `splash-sandbox`
too. So "run bash" becomes *a named, bounded, audited, sandboxed executable tool*,
never a raw shell. That is the point, not a limitation.

ymote/Splash's own positioning agrees: it is **not** a Python/JS replacement and
provides *"no standard filesystem/network/process API"*; it aims at the
*"constrained orchestration niche … host-controlled effects"* (`docs/positioning.md`).

---

## 6. Ambient bash vs. the capability model — a fit judgment

Today's coding agents (Claude Code and peers) call bash directly or shell out from
a small script. **That is not wrong — it fits their environment:** a trusted
operator, on their own machine, on their own files, approving each step and able
to undo. Ambient bash there is pragmatic; a capability system would be friction
for no benefit.

The trade flips when the assumptions flip:

| context | ambient bash / little script | capability-scoped model |
|---|---|---|
| local dev, you drive it, reversible | ✅ ideal | ❌ overkill |
| **untrusted author** (LLM-generated tool code) | ❌ arbitrary effects | ✅ deny-by-default fits |
| **high-security / multi-tenant** | ❌ no least-privilege / audit | ✅ scoped, bounded, audited |
| **effects must be exactly-once** (pay, publish) | ❌ retries double-apply | ✅ the ledger (§4) |
| **mobile / embedded** | ❌ often *impossible* | ✅ the realistic path |

**Mobile is the strongest case, and it is not merely "nicer."** iOS and Android
do not give an app a shell; spawning arbitrary processes is blocked by the OS and
forbidden by store policy. The "write a python script to shell out" pattern that
works on a laptop **does not port to a phone at all.** The realistic path on a
device is exactly this model: the native app links a small set of vetted
operations and hands the generated logic capability-scoped access to them. The
docs call this *"useful catalog governance for mobile and embedded applications"*
and dispatch *"bounded for mobile or embedded event loops."*

For octos-one specifically — LLM-composed cards, on a phone, that will eventually
perform real effects — this is exactly the intersection where the capability model
is not just plausible but close to mandatory.

---

## 7. Recommendation for octos-one

1. **Treat effects and orchestration as different layers.** Keep DOT for
   orchestration; adopt `splash-workflow` as the *effect/authority* layer under
   the cards. Do not expect one to do the other's job.
2. **Gate the migration on the effect roadmap.** If cards will perform real,
   irreversible, must-survive-crash actions, the exactly-once + capability +
   sandbox story is a genuine, un-had benefit and worth the VM re-bind (see
   [SPLASH-VS-TAURI.md](SPLASH-VS-TAURI.md) §6). If cards stay display/animation,
   defer — the machinery buys safety you are not exercising.
3. **Never give cards ambient bash/FS/network.** On-device, expose a *small set*
   of vetted native operations as capability-scoped tools (`Executable` /
   `FileRoot` / `NetworkOrigin` / `Secret`), run them through `splash-workflow`
   for exactly-once, and contain them with `splash-sandbox` where an OS sandbox
   applies. This is also the only model that ports to iOS/Android.
4. **Prove it on one real flow first.** Take the single highest-value
   side-effecting card, implement it end-to-end on lease → ledger →
   dispatch → reconcile → checkpoint → compensate, including a killed-worker
   reconcile, before migrating the rest.

---

## Provenance

Documented from a working session on 2026-07-28. The design-goal claims in §3–4
were verified by compiling and running the real `splash-workflow` crate
(ymote/Splash @ `623d57c`) through its public API and diffing against an
independent Python baseline; the derivation scheme was cross-checked with a
byte-for-byte Python reimplementation. Tool-model claims in §5–6 are from the
ymote/Splash docs (`external-tools.md`, `positioning.md`, `fixed-file-catalog.md`,
`http-endpoint-catalog.md`) and `splash-protocol` / `splash-sandbox` source. Line
numbers are anchors against `623d57c`, not contracts — they drift with edits.

**Open items:**
- The runnable validation harness is not committed here; decide whether it belongs
  under `octos-one` (e.g. `docs/examples/splash-workflow-validation/`).
- §7 step 2 (migration gate) depends on a product decision — will cards perform
  real external effects? — that is not yet recorded in the repo.
- The exactly-once contract requires a worker that consults the ledger before
  acting; octos-one has no such worker adapter yet (§4 boundary).
