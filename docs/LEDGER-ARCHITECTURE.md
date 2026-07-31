# The Ledger, the Host Surface, and the Splash Repository

**Status:** **hypothesis document, not an approved architecture.** Revision 7.1.

> **R7.1 is a corrections pass following external review.** No new claims. Three numeric
> errors are fixed, five overclaims are retracted, and the measurement basis is qualified.
> Counts are now produced by `docs/tools/card-metrics.sh` rather than asserted — R4–R7
> contained three separate comment-contaminated greps, and the same broken method survived
> a round in which one of its columns had already been corrected.
**Scope:** what the LLM emits, what the runtime realizes, where that code lives, how three
backends stay at parity, and what the inference-side cache actually does.

> **Revision history.**
> **R1** claimed a security model and durability protocol had to be built from scratch.
> Wrong — Splash specifies both.
> **R2** overcorrected into borrowed confidence and got the cache mechanism wrong.
> **R3** retreated: concluded the cache benefit was unrealizable.
> **R4** reversed that on measurement — 96.9–99.0% across three providers.
> **R5** separated lifecycle from payload, but conflated *format* with *decidability*.
> **R6** corrected that: decidability comes from the grammar, not the syntax.
> **R7** adds the **component model with declared local state** (§4.3), which closes the
> composition gap R6 left open, and corrects two factual errors: nav has **zero** `while`
> loops (all three hits were comments), and nav's L2 requirement is a **reconciliation**
> problem, not an expressiveness one (§4.5).

---

## 1. The scenario this exists to serve

Traditional apps spend most of their UI budget *acquiring intent*. Search boxes, filters,
faceted lists, category trees — nearly all of it exists because a deterministic program
cannot resolve "something to watch tonight" into a query.

An LLM can, and it holds the context that makes the resolution *personal*: preferences,
memory, and live signals. It can act before it is asked:

> *"I feel tired"* → a film that fits the user's taste and the hour
> — or, noticing a friend is in town, a suggestion to see them instead.

| # | Requirement | Consequence |
|---|---|---|
| R1 | Express new experiences, not rearrangements of stock widgets | The emission must be a *language* |
| R2 | Improve incrementally, unsupervised, during idle time | The emission must be a *durable artifact* |
| R3 | Stay live without the model re-running | Data and interaction must be *declared dependencies* |
| R4 | **Compose** services, not just render one card | The language needs **components** (§4.3) |

R4 is new in R7. §1's premise — "compose a personal service" — was never expressible in a
flat global namespace.

---

## 2. Two orthogonal decisions

| Decision | Options | Status |
|---|---|---|
| **Lifecycle** | append-only sealed segments + shadowing fold | **stable-prefix wire feasibility demonstrated (§9); lifecycle unresolved (§8.2)** |
| **Grammar** | how much of the language a given card may use | proposed, unspecified (§4.4) |

*R7 called the lifecycle "settled." It is not: §9 measures that a stable prefix caches and
that mutation destroys it, which is a wire-format result. Storage layout, recovery and
garbage collection remain undesigned (§8.2).*

### 2.1 Why not JSON — measured

A2UI (pinned to v0.9) has flat ID-addressed component updates, reactive data binding,
dynamic list templates and registered function values. **For a weather, news or stock card,
our `view` records and A2UI's component tree describe the same thing** — two independent
derivations landing on the same shape, which is evidence the shape is right. Their component
vocabulary and binding semantics are worth borrowing from.

We do not adopt it as the wire format, for two measured reasons.

**Density.** The same card — hero, 7-day keyed loop, three tiles, tap-to-toggle — measured
with the provider's own tokenizer:

| | Tokens |
|---|---|
| A2UI-shaped JSON | **577** |
| Restricted Splash | **262** |
| | **2.20× denser** |

The overhead is structural: binding wrappers (`{"$bind":"/now/temp"}` vs `now.temp`), an
explicit id on every node where the declaration path already names it, `"type"`/`"props"`
framing, and template indirection where a loop body must be hoisted into a named component.

**And density lands where it costs most.** From §9.6, output tokens are never cached and
decode runs ~91 tok/s against prefill's ~12,700. A verbose baseline is nearly free once
cached; a verbose *emission* is paid on every edit forever:

```
A2UI  577 tok → 6.3 s      Splash  262 tok → 2.9 s      3.5 s saved per refinement
```

**Expressiveness.** `trip-planner.splash` has 127 conditionals and a `fn tick()`. JSON
doesn't merely get verbose there — enumerating variants blows up combinatorially, and
registering client functions moves the logic into the client catalog, so the agent can no
longer author new behaviour, contradicting R1.

Counts below are comment-stripped, produced by `docs/tools/card-metrics.sh`:

| Card | `if` | `for` | `while` | `fn` | imperative `ui.*` calls |
|---|---|---|---|---|---|
| weather / news / stock | — | — | — | — | — |
| `navigate.splash` | 28 | **0** | **0** | 1 | 11 |
| `trip-planner.splash` | 127 | **0** | **0** | 1 | 65 |

*R4–R7 reported loop counts of 1–3 for these files. **Every one was a comment** — "while the
fetch is loading", "results for the typed query". **Neither nav card contains a loop of any
kind.** This weakens the argument above: the case against JSON here rests on conditionals and
a `fn`, not on iteration.*

### 2.2 Why React-shaped, not React

Splash has two audiences — humans and models. The argument for familiar idiom is that an LLM
emitting it draws on a large training prior, so first-pass accuracy should be higher and
repair round-trips fewer.

**This is a hypothesis, not a result.** Nothing here measures it. The literature supports
only the weaker claims that unfamiliar DSLs are hard and that familiar representations
*sometimes* help when paired with constraints and repair — and separately that models
over-select familiar languages even when inappropriate, which makes familiarity a source of
bias as well as accuracy. The "1.2–1.4×" density figure for JSX-style syntax was an estimate
with no tokenizer behind it; only the JSON comparison in §2.1 was measured.

Settling it needs a same-task A/B: parse success, conformance-test pass rate,
forbidden-construct emission, repair rounds, edit correctness, output tokens.

But we take React's *shape*, not its machinery:

| Adopt | Reject |
|---|---|
| Component model, props, local state | **Hooks and their call-order rules** — undecidable, and a trap for a model |
| Keyed lists | `useEffect` + dependency arrays — declared `source` deps are strictly better |
| View as a pure function of state | `useRef` / imperative escapes — that is L2 |
| | Context — reintroduces the invisible dependencies §6.2 warns about |

**The closer reference is SwiftUI or Svelte 5 runes, not React proper**: state is *declared*
(`@State var x = 0`) rather than *called* through a hook, the framework owns storage and
identity, and the view is a pure computed property. That maps onto the ledger almost
exactly — React's shape without React's call-order semantics.

**Decision on record:** octos-one targets one cross-platform backend for card rendering.

---

## 3. What the ledger is

> A single append-only document, per app, authoritative for the app's **generated
> declaration layer**.

Not "the program" — nav's imperative behaviour, the widget set and the capability runtime
remain backend code. Not state, and not what the user saw: reproducing a screen needs the
ledger **plus** source values, persisted state, runtime version and capability outcomes.

It is a file the LLM **maintains**. Scanning tens of KB and changing a few lines is reliable;
regenerating 664 lines to change one is not. **Regeneration drift** — one fix silently moving
unrelated things, 45–90 s, no reviewable diff — was measured repeatedly. §9.6 shows the
45–90 s was decode throughput, and therefore predictable.

### 3.1 Append-only, via sealed segments

1. **Audit** — history survives, so unsupervised refinement is reviewable.
2. **Edit size** — the change is the diff, and so is the review.
3. **Cache** — measured at 96.9–99.0% prefix reuse across three providers (§9).

```
seg-0001 (sealed) ─► seg-0002 (sealed) ─► HEAD (open, appended)
                            │
                      checkpoint-0002
```

Sealed segments are never rewritten and hash-link their predecessor. Compaction seals the
open segment and emits a checkpoint; superseded segments are archived, not deleted. Rollback
moves HEAD or appends a compensating record — never truncation.

**Hash-linking alone is not tamper-evidence.** It detects edits within a chain, not
replacement of the whole chain with an older internally-valid one. That needs a trusted
monotonic root (§8.3), which does not yet exist on our targets.

### 3.2 The wire representation is load-bearing

**A sealed segment serializes as its own message. Appending adds a message; it never grows an
existing one.** Measured at 99% versus 0% (§9.2).

---

## 4. Records, components, and capability levels

### 4.1 Records

| Kind | Contents |
|---|---|
| `source` | a declared capability dependency: name, args, fields |
| `state` | shape + initial value |
| `event` | name → set of transitions |
| `derive` | pure expression over sources and state |
| `component` | reusable unit: props, local state, local events, view (§4.3) |
| `view` | component tree, bindings, keyed loops, guarded branches |
| `copy` | literals + provenance class (§5.3) |

Meta: `checkpoint`, `retract`, `note`.

All records are Splash. What differs between cards is **how much of the grammar they may
use** (§4.4).

### 4.2 The immutable/mutable split

This is the invariant the whole design rests on, and components extend it rather than
complicate it:

> **The ledger declares shapes. The runtime owns values, events and data.**

The ledger is immutable and append-only. It never contains a temperature, a scroll offset, a
toggle's current value, or a fetched headline. It contains the *shape* of those things and
the *queries* that produce them. The runtime injects the rest at realization.

### 4.3 Components with declared local state

A flat global namespace cannot express composition (R4). Components fix it without adding
computation:

```
component ForecastRow(day) {
  state  expanded = false                    # shape + initial — immutable in the ledger
  on     toggle   { expanded: !expanded }    # declared transition
  view   Row { Text(text: day.dayname)
               TempBar(lo: day.lo, hi: day.hi)
               when expanded { Detail(day: day) } }
}

view forecast  Panel { for d in week key d.dayname { ForecastRow(day: d) } }
```

Seven rows, seven independent `expanded` values — **none of which appear in the ledger.**
The runtime stores them keyed by instance identity.

**That identity is underspecified, and R7's `(declaration path, loop key)` is not "React's
rule."** React associates state with position in the *realized* tree, where component type
and keys both affect preservation. A workable tuple needs at minimum the card namespace,
parent instance identity, instantiation site, component type and version, and the full vector
of enclosing keys — plus defined behaviour for duplicate keys, nested loops, branch
replacement and component redefinition. Concretely: two `TripPanel` instances each containing
an unkeyed `Toggle` must not share one state cell, and a static declaration path would make
them do exactly that.

Four things improve at once:

| | Flat records (R6) | Components (R7) |
|---|---|---|
| Composition | global paths only | props + local state, nests properly |
| Reconciliation granularity | bounds at the **record** | bounds at the **instance** |
| Emission density | repetition written out | define once, instantiate N times |
| Overlay targeting | shadow a whole view record | shadow one component definition |

The last one matters for the app-store model: a personal overlay can redefine `ForecastRow`
without touching the baseline's `forecast` record, so the diff stays tiny and the cached
prefix survives.

**R7 claimed "no computation was introduced." That was false** — `expanded: !expanded` is
computation, one line below the claim. Declared storage does not make a transition total or
bounded, and unbounded numeric or string state, recursive components, and event cascades
between components are all reachable from this grammar.

What can honestly be said: transitions are *syntactically confined* to a declared form, which
is a precondition for bounding them, not a proof that they are bounded. §4.4 states what
remains to be specified.

### 4.4 Capability levels are grammar restrictions

| Level | Grammar admitted | Host surface at eval |
|---|---|---|
| **L0** | constructors, bindings, keyed loops, guarded branches, components with declared local state and transitions. No host calls, no free computation, no unbounded loops | **empty — zero capabilities** |
| **L1** | + pure expressions in `derive` | empty; sources injected as data |
| **L2** | full Splash, incl. imperative widget commands | §7 containment required |

**"Decidable" was used loosely in R5–R7 and is withdrawn as a blanket claim.** It names at
least six different properties, which do not stand or fall together:

| Property | Status at L0 |
|---|---|
| Grammar membership | decidable, once a normative AST exists |
| Contains no authority-bearing operation | decidable, by transitive effect check |
| Termination / boundedness | **not established** — needs total transition forms and collection/depth limits |
| Dependency closure | plausible from structurally visible bindings; unproven |
| Fact provenance (§5.3) | decidable only for literals in typed data positions |
| Reachable state space | **not established** |

What L0 is intended to buy is the second row: **authority confinement**. That is a real and
useful property, and it is weaker than "decidable."

Determining a card's level cannot be three token tests. R7's rule —

```
any record defines a fn, loops unboundedly, or calls ui.<id>.<method>  →  L2
any derive computes                                                    →  L1
otherwise                                                              →  L0
```

— is both incomplete and self-contradictory: it classifies `!expanded` as L0 while assigning
computation to L1, recognises one widget-call shape but no other calls, imports or callable
values, and inspects records locally. A card instantiating a catalog component whose
definition is later replaced by an L2 one still reads as L0 unless component versions are
pinned and levels resolved **transitively**. Level must be an effect judgment over the
transitive component closure.

### 4.5 Nav is a reconciliation problem, not an expressiveness one

R4–R6 treated nav as the case that proves L0 insufficient. R7 overcorrected to "purely a
reconciliation problem." **Both are wrong; reconciliation is necessary but not sufficient.**

**Corrected counts** (comment-stripped, `docs/tools/card-metrics.sh`) — trip-planner has
**65** imperative calls, not the 37 R7 reported:

| Call | trip-planner | navigate | Purpose |
|---|---|---|---|
| `ui.*.set_text` | **53** | 6 | update a label in place at ~1 Hz |
| `ui.*.set_visible` | 3 | 3 | show/hide bypassing state |
| `ui.*.set_nav_polyline` | 3 | 1 | push route geometry into the map |
| `ui.*.set_route_markers` | 3 | 1 | push markers into the map |
| `ui.*.nav_zoom_by` | 2 | 0 | drive the camera |
| `ui.*.nav_center_origin` | 1 | 0 | drive the camera |
| **total** | **65** | **11** | |

**The conditionals are also not all simple equality on declared state**, as R7 claimed. Many
are — `if find == "stop"`, `if scr == "drive"` — but others are numeric checks on source
results, range checks over derived locals, and predicates over a computed routing mode.

**The diagnosis that survives** is about the map's private state: `MapView` retains camera,
gesture, animation, route, label and tile state, a Path B rebuild destroys all of it, and the
runtime explicitly suppresses nav rebuilds for that reason. `sys.navsecs` exists to provide a
clock that avoids triggering it.

**What R7 ignored is the rest of `fn tick()`:** mutable route assembly, arithmetic, source
polling, late-data synchronisation and clock-driven progression. Under §4.4's own rule, a
`fn` puts a card at L2 — so **nav remains L2 even if every widget command becomes
declarative.** Replacing `tick`'s temporal logic with declared sources and derives is an
unsolved design problem, not a mechanical translation.

**And a declarative `MapView` is not sufficient by itself:**

- Route and marker values can be idempotent desired-state properties.
- **`zoom_by` and `recenter` are commands, not state.** Reapplied during reconciliation a
  delta repeats; modelled as an absolute property it overwrites the user's gesture. Concrete
  failure: the user pans, a 1 Hz tick reconciles `zoom: 15`, and the camera snaps back every
  second.
- The card builds *different* `MapView`s in its 2D/3D branches, so mode switching unmounts
  the map and loses private state unless branch identity and type-change behaviour are
  specified.
- Async route and tile completion need generation ownership and stale-result handling.
- Controlled, uncontrolled and initial-only properties are undistinguished.

`docs/CARD-STATE-IDENTITY.md` is more conservative than R7 was, and correctly so: nav should
retain its hand-written tick/map path until this is proven.

### 4.6 Fold semantics

Last write per path wins; `retract` removes. The fold is deterministic: the same chain yields
the same declaration table. It does **not** follow that the same table yields the same pixels.

---

## 5. No facts in the ledger

**A ledger entry may contain a question. It may never contain an answer.**

### 5.1 Why — six bugs, one cause

| Symptom | Root cause |
|---|---|
| Plausible but wrong weather condition | model authored `condition:` |
| Forecast bar scaled against invented bounds | model authored `wmin`/`wmax` |
| Day labels drifted from the forecast | model authored weekday names |
| Card geocoded to the wrong city | model authored lat/lon |
| CJK rendered as tofu boxes | model authored `font_family` |
| Sibling top-level nodes split the card | model authored the root |

Each was fixed by **deleting the field**.

### 5.2 Stricter under a ledger

A regenerated card is freshly wrong; a persisted one is wrong forever, and the next editor
cannot distinguish a stale value from a deliberate choice. `wmin: 10` reads as intent.
`sys.weekmin(...)` cannot rot.

### 5.3 Enforcement, by grammar level

Every literal carries a **provenance class**:

| Class | Origin | May an LLM create it? |
|---|---|---|
| `source-derived` | a capability result | n/a — not a literal |
| `vocabulary` | trusted static set | no — host-owned |
| `user-copy` | authored by the user | no |
| `model-copy` | authored by the model | yes, **never in a data position** |

**At L0 this is decidable**, because the grammar makes a `view` a typed tree. The plan layer
already demonstrates the strongest form — removing the `condition` field made that bug
*unrepresentable* rather than forbidden.

**At L1/L2 it is not.** `trip-planner.splash:234`:

```
ui.remrest.set_text(sys.navstep(…, "rem", vias) + "  ·  34 mph")
```

A model-authored fact concatenated onto a live call, shipping today.

So: **decidable at L0; narrowed by Language Profile v0.2 at L1; review plus residual risk at
L2.**

---

## 6. Reactive realization

### 6.1 What exists today

Two disjoint paths (`docs/CARD-STATE-IDENTITY.md`, itself unimplemented): **Path A**
(`ui.<id>.set_*`, hand-wired, zero rebuild) and **Path B** (state write re-evaluates the whole
body). On the standard ladder — full re-render → VDOM diff → fine-grained signals — Splash
sits at rung 1. There is no reactive system.

### 6.2 Dependency tracking

**Under-invalidation is the dangerous direction** — over-invalidation degrades to today's
behaviour; under-invalidation shows wrong data. Sources: implicit dependencies (theme, locale,
clock, permissions, viewport), reads inside uninstrumented host calls, async staleness.
Mitigation: implicit dependencies become **declared sources** (`source env.locale`).

**Dynamic tracking cannot reject cycles at append time:**

```
a = if use_b { b } else 0
b = a + 1
```

With `use_b = false` a trial records no `a → b` edge and passes. Append-time rejection needs a
**statically declared dependency superset**, with tracking narrowing invalidation within it.
Declaration is the contract; tracking is the optimization.

**L0 makes this easy:** its grammar has structurally visible bindings, so the dependency set
is readable without evaluation.

### 6.3 Identity and granularity

Instance identity is `(declaration path, loop key)` — never tree position. Explicit keys are
required at loop items (keyed on the item, not the index) and conditional branches.

**With components, granularity bounds at the instance**, not the record — React's granularity,
reached through signals rather than a VDOM diff. A 200-node view no longer re-realizes because
one row's `expanded` flipped.

### 6.4 What is still missing versus React

Honest gap list, all unbuilt: `useEffect` equivalent (today `fn tick()` is manual polling),
memoization (`derive` would be it), async/Suspense boundaries (proposed as pending/stale/error
view states, §13), error boundaries (proposed as last-known-good fallback). Components close
the largest gap — local state and composition — but not these.

---

## 7. Security

### 7.1 The live hole

Every card VM registers the full platform module.
`aichat/widgets/src/widget_async.rs:274` → `platform/src/script/mod.rs:14` →
`makepad_script_std::script_mod` →

| Module | Reaching cards | Impact |
|---|---|---|
| `fs` | `read`, `read_to_string`, `write`, `write_string` | arbitrary read **and destructive write** |
| `run` | `run`, `child` | child process spawn |
| `net` | `http_request`, `http_server`, `socket_stream`, `web_socket` | arbitrary egress, listeners |
| `cx` | `quit` | kill the app |

`runsplash_body_forbidden` (`main.rs:1985`) blocks five strings, none of them `fs`, `run`,
`quit` or `http_server`. Its own comment concedes: *"NOT a hard boundary … the real fix is
VM-level capability gating."*

Concrete: a card reads a token with `mod.fs.read_to_string`, concatenates it into
`sys.photo(…)` — which percent-encodes arbitrary text into an outbound URL — and the image
loader fetches it. Passes the scanner.

**The fix is non-registration.** Verified safe: zero uses of `fs.`/`net.`/`run.`/`.quit`
across every `.splash` in the repo including the framework's own.

| # | File | Change |
|---|---|---|
| 1 | `platform/script/std/src/lib.rs` | `script_mod_sandboxed` — `task` only |
| 2 | `platform/src/script/cx.rs` | sandboxed variant — `os_type`, not `quit` |
| 3 | `platform/src/script/mod.rs` | compose: sandboxed cx + std + `timer` + `res` + `draw` + `event` |
| 4 | `widgets/src/widget_async.rs:274` | `alloc_splash_vm` calls the sandboxed variant |
| 5 | `app/app/src/main.rs` | keep the scanner as a **lint**, not a boundary |

**This is the L2 host surface.** L0 and L1 get an empty one (§4.4).

### 7.2 Per-card isolation rests on a string replace

`widgets/src/lib.rs:669` gives every isolate `agent.notify`. But:

```rust
fn tag_notify_calls(body: &str, item_id: usize) -> String {
    body.replace("agent.notify(\"", &format!("agent.notify(\"{item_id}:"))
        .replace("agent.notify('", &format!("agent.notify('{item_id}:"))
}
```

**Event attribution is a source-text rewrite.** It matches only a literal-string first
argument, so a card building its event id in a variable escapes tagging.

**Corrected from R7:** that revision called this a *state isolation* failure. It is not.
Per-card state writes are already gated on an attributed `card_id`, so an untagged event
resolves to `None` and **fails safe** — it cannot reach another card's state. What was
genuinely exposed was one branch that ran *before* that gate and would navigate on an
unattributable event; it is now gated too, and an untagged notify is logged and dropped.

The residual is that dispatch matches event names by substring, so `inc` also matches
`since`. That leniency looks deliberate — it absorbs model variation in event naming — and
tightening it risks breaking working cards, so it is recorded rather than changed.

The class of mistake still stands: a textual guard on a language that can compute. Binding
identity in the runtime, from the card's own VM, remains the right fix.

**This does not exist at L0**, where `on` is a declared transition with a name the grammar
fixes. Event identity should be bound by the runtime at dispatch for all levels.

### 7.3 The posture each level carries

| Level | Reachable capabilities | Containment required |
|---|---|---|
| L0 | none — grammar admits no call | **authority confinement structural; bounded evaluator still mandatory** |
| L1 | none — pure expressions only | same, plus expression bounds |
| L2 | full host surface | everything in §7.1, §7.5, §7.7 |

**R7's "containment required: none" is withdrawn.** L0 source is still evaluated by a VM, and
that evaluation still depends on heap, stack, call-depth, instruction and time bounds, plus a
trusted validator and lowering boundary. The correct statement is narrower and still useful:
*L0 needs no OS authority boundary, because no authority-bearing operation can be written —
but evaluator resource containment is not optional at any level.*

### 7.4 Correcting R2's threat state

R2 claimed cards run with no execution limits. **False for the shipping path.**
`splash.rs:2884` wraps evaluation in `with_instruction_limit(SPLASH_EVAL_INSTRUCTION_LIMIT, …)`
and `widget_async.rs` applies a 64 ms wall-clock budget. Only `app/splash-native` builds a raw
VM with unrestricted `eval`.

### 7.5 What Splash provides

Two boundaries (language: no ambient APIs; execution: contains adapters with OS effects).
Uncatchable limits — `try/catch` "cannot catch string-allocation, heap-allocation,
operand-stack, call-frame, instruction-limit, or hard-deadline termination."
`CapabilityLease` binds tools and budgets to the exact catalog fingerprint. `WorkflowDraft` is
a bounded untrusted input format for LLM-proposed steps. Review aids are explicitly **not**
authorization.

R2 overstated exfiltration prevention: `HttpEndpointCatalog` fixes destinations, but Splash
notes POST-body semantics may need separate review and the catalog constrains neither other
adapters nor the OS. The **exact-origin catalog** is the better fit for `sys.photo`.

### 7.6 The structural hole: no LLM-safe UI path

```rust
Runtime::eval(source)                  // canonical v0.2 — safe for LLM source
                                       // …but check_syntax REJECTS UI syntax
Runtime::eval_vm_compatibility(source) // accepts UI syntax
                                       // …"must not receive LLM-generated or
                                       //    otherwise untrusted source"
```

So **Splash has no supported path for LLM-generated UI**, and octos-one occupies that gap by
bypassing Splash — which is how it inherited `fs`/`run`/`net`/`quit`.

R2's "wiring gap, not a design gap" is withdrawn: `Runtime`'s VM accessor is private
(`splash-core/src/lib.rs:1200`), the renderer needs a live `ScriptVm` after evaluation, and
re-registering the platform module through `Runtime::configure` would reinstall the APIs
Runtime masks.

**The L0 grammar is the first and smallest increment of that profile.**

### 7.7 Remaining gaps

1. **No mobile execution boundary** — gates **L2** with private data.
2. **Prompt injection into idle refinement** — source content is data, never authority;
   refinement runs with a narrower lease; source-declaration changes need approval.

---

## 8. Durability

### 8.1 What `splash-storage` is

A **host-only record boundary**, not a ledger protocol. `AuthenticatedStore` seals a payload
with a 32-byte BLAKE3 key binding envelope version, namespace, name, key ID and revision.
`RollbackProtectedStore` adds compare-and-swap against a durable revision floor and fails
closed on both rollback and anchor desync.

**R2 called this "the commit protocol." It is not.** The API is per-`StorageRecordKey` CAS with
a **256 KiB payload limit** — no segment-chain API, multi-key transaction, HEAD promotion,
archive, enumeration or recovery algorithm.

### 8.2 The unresolved fork

| Option | Consequence |
|---|---|
| Whole chain in one record | every append rewrites the payload; 256 KiB ceiling; physical append-only disappears |
| Segments + HEAD under separate keys | no atomic multi-key transaction — a crash between segment write and HEAD promotion orphans it |

Neither is chosen, and **recovery and garbage collection are undesigned**. The largest
unresolved item in the document.

### 8.3 Three corrections

- **CAS detects conflicting writes; it does not enforce a single writer.** Splash has a
  separate fenced-writer contract requiring host admission and monotonic fencing tokens.
- **Moving HEAD backward is not auditable** unless HEAD transitions are retained in
  authenticated history.
- **Trial realization cannot establish "never a dead app."** A dangerous branch may run only
  after a click, permission transition or async completion.

**The mobile rollback anchor remains unidentified.**

---

## 9. Cache economics — measured

Nothing here depends on grammar level.

**What these numbers are, and are not.** They are cached-token *fractions* for one prompt
shape, from single runs against live APIs, with no repetitions, distributions, confidence
intervals, TTL-expiry tests, routing tests, concurrent cold-start tests or production traces.
They demonstrate a **mechanism** — a stable prefix caches, mutation destroys it, the saving
falls on prefill — not a hit *probability* over time or under load. The scripts currently live
in a session scratchpad and are **not yet reproducible in-repo**; they should be checked in
alongside `docs/tools/card-metrics.sh`.

### 9.1 The one rule

> **Append new messages. Never grow an existing one.**

### 9.2 Measured

| Provider | Mechanism | Cold | Repeat | **Append** | Append ×2 |
|---|---|---|---|---|---|
| **OpenAI** (gpt-4o-mini) | token prefix | 0% | 99.3% | **98.2%** | — |
| **GLM-5.2** (z.ai) | message prefix | 0% | 99.5% | **99.0%** | **98.9%** |
| **Kimi K3** (moonshot) | message prefix, 256-tok blocks | 0% | 99.4% | **98.2%** | **96.9%** |

Negative control, every provider: editing **one word** ~5 records into 260 → **0%**. Growing
the system message in place → **0%**.

**R3's negative result was a broken test** — it grew a message rather than appending one.

### 9.3 Shared baseline across users

GLM, same baseline + different user requests: **99.5%** each. OpenAI, second user's overlay:
**98.2%**. One write, many readers; the personal delta belongs in the *uncached* tier.

### 9.4 Provider tuning

- **GLM extends** its cached prefix on each append (13,760 → 13,824 → 13,952).
- **Kimi held at 12,800 cached tokens across appends.** R7 read this as 256-token
  quantization (12,800 = 256 × 50). That is an *inference from one observation* — Moonshot
  documents a 256-token minimum for a prompt to seed a cache, not a block size. Consistent
  with, but not established by, the data.
- **Placement differs**: GLM prefers `system`; Moonshot recommends the head of `messages`.

### 9.5 Cost

| Provider | Fresh | Cached | Discount | Write premium |
|---|---|---|---|---|
| Anthropic Opus 5 | $5.00/M | $0.50/M | 90% | 1.25× (5m) / 2× (1h) |
| Kimi K3 | $3.00/M | $0.30/M | 90% | none |
| GLM-5.2 | $1.40/M | $0.26/M | 81% | none |

Per request (Opus 5): **$0.0638 → $0.0068, 89% saving.** Break-even is under one read **at the
5-minute write premium**; the 1-hour premium (2×) needs more. The `~$63,800/day → ~$6,800/day`
figure is an **input-only projection** assuming a continuously warm cache and near-perfect
reuse — an upper bound on the saving, not an observed cost.

### 9.6 Latency: prefill only

| Output | TTFT cold | TTFT warm | Total cold | Total warm | Saving |
|---|---|---|---|---|---|
| 16 tokens | 1.78 s | 0.79 s | 2.02 s | 1.02 s | **−50%** |
| 400 tokens | 1.40 s | 1.03 s | 6.04 s | 5.20 s | −14% |

Decode unchanged (0.24 vs 0.23 s; 4.64 vs 4.17 s). Prefill ~12,700 tok/s, decode ~91 tok/s —
**decode is ~140× slower per token.** This is why §2.1's density ratio compounds: the cache
cannot touch output.

### 9.7 Cache-key fragmentation

Moonshot documents two hard invalidation triggers: **switching model** and **switching
`reasoning_effort`**. So **model and effort tier are part of a baseline version's identity.**

### 9.8 What the literature adds

*Do LLMs Need a Content Delivery Network?* (HotInfra'24): **2.5× cheaper, 3.7× faster than
in-context learning** — from its Table 1, in-context $0.0149 / 10.91 s versus KDN $0.0059 /
2.97 s. *(R4–R7 reported these swapped.)* Its mechanism note stands: **KV size grows linearly
while prefill delay grows superlinearly**, so larger shared baselines pay better. Note this is
a self-hosted KV-cache system, not a hosted provider's prompt cache — the mechanisms differ. Their cited multipliers are optimistic — CacheBlend is
**10–20% recompute** and **2.2–3.3× TTFT** (not 10×); CacheGen alone is **3.5–4.3×** (not >10×,
which stacks token-dropping and KV eviction — corruption, not compression, for a ledger).
**We need blending less than RAG does**: a ledger has stable ordering where the baseline is
both prefix and largest chunk.

### 9.9 What this does not do

Cross-model KV transfer is closed off twice: dimensionally undefined outside its own model, and
~14,000–80,000× larger than its source text. Same-model transfer needs the same weights
resident. The only decoder that fits a phone has **zero weights**.

---

## 10. Division of responsibility

| Concern | Owner |
|---|---|
| Which entity the user meant; composition; preference and memory | LLM |
| Card structure, ordering, emphasis — **within a style contract** | LLM |
| Style tokens, layout, theme, fonts | runtime |
| Fetching, retry, permissions, units, locale, formatting | runtime |
| **All state values, including component-local** | runtime |
| Diffing, identity, reconciliation | runtime |
| Every fact | runtime |

---

## 11. Repository structure

```
Splash/
├── crates/                        workspace 1 — no UI deps, security-critical
│   ├── splash-core                  VM, grammar, Runtime      [exists, 12,658]
│   │     └── profile/ui             NEW  UI grammar: L0 subset, then L1, then L2
│   ├── splash-schema / -storage / -protocol / -worker         [exists]
│   ├── splash-capabilities / -workflow / -sandbox / -linux-*  [exists]
│   ├── splash-cli / -lsp                                      [exists]
│   ├── splash-ui-host             NEW  host surfaces per level (empty for L0/L1)
│   ├── splash-app-catalog         NEW  sys.* + sensors, declared once
│   ├── splash-ledger              NEW  segment chain, fold, level + instance state
│   └── splash-mobile-containment  NEW  gates L2 on mobile
│
├── ui/                            workspace 2 — the shared UI spine
│   ├── splash-render                GROWS: + layout, text roles, theme
│   ├── splash-widgets               widget semantics, not realization
│   ├── splash-backend             NEW  the port trait (§12)
│   └── conformance/               NEW  card corpus + golden output
│
└── backends/  splash-makepad · splash-android · splash-oh
```

**Two workspaces**, so `makepad-widgets` never enters the tree `splash-sandbox` and
`splash-storage` live in. **The UI profile is a module in `splash-core`**, since it shares the
parser and `Runtime`'s VM accessor is private.

**`splash-app-catalog`** matters disproportionately: `sys.*` is hand-written four times today
(aichat 42 helpers, splash-native 21, Splash-Android 8, Splash-OH 12) and **every divergence
has surfaced as a device-visible bug.**

---

## 12. Three backends at parity

| | Lines | Note |
|---|---|---|
| `splash-render` | **824** | shared core; no makepad deps |
| `splash-makepad` + `splash-widgets` | 3,136 | the Makepad backend |
| `splash-oh` + `splash-oh-native` | **15,207** | ~5× Makepad |
| Splash-Android | **17 files** | barely started |

OH's excess is not rendering — a **capability layer** (~2,400 lines) and an **app layer**
(~2,000 lines) that belong elsewhere.

Three structural moves: **grow the spine** (layout, text roles, theme are re-derived per
backend today and drift visibly); **shrink backends to a ~5-operation port**
(`realize`, `measure_text`, `load_image`, `deliver_event`, `draw_primitive`); **enforce with
conformance** — one corpus, golden output per backend, plus a capability manifest so a missing
helper fails loudly instead of rendering an em dash.

**L0 helps parity directly:** its grammar lowers to `UiNode` with an empty host surface, so
backends agree on a tree rather than on evaluation semantics — already proven by
`plan/nodes.rs`. **And `MapView` needs a declarative interface anyway** (§4.5), which is the
same work.

Cost: Makepad donates upward; OH sheds ~4,400 lines; **Android is built from scratch.**

---

## 13. Operational design

| Concern | Requirement |
|---|---|
| **Atomic activation** | draft → validate → trial-realize → CAS → HEAD advance, plus last-known-good at realization |
| **Transactions** | multi-record refinement is all-or-nothing |
| **Concurrency** | fenced single writer (§8.3), not CAS alone |
| **Multi-device sync** | **unresolved** — single-device until designed |
| **Schema evolution** | unknown record kinds **block activation** absent an explicit compatibility declaration |
| **Level enforcement** | checked at append; widening requires explicit promotion |
| **Instance state evolution** | **NEW and unresolved.** When a component definition changes — by overlay or version bump — what happens to live instances holding local state? Does `expanded` survive a field addition? Does a changed loop key re-key or reset? Per-instance state makes §13's old flat-namespace answer insufficient, and it interacts with §9.7's version-bump invalidation |
| **Source lifecycle** | pending/stale/error are first-class view states |
| **Evaluation failure** | last-known-good with a visible diagnostic — never a blank card |
| **Compaction correctness** | immutable archive, crash-safe head replacement, fold-equivalence check |
| **Wire form** | one message per sealed segment; appends never mutate an earlier message |
| **Cache telemetry** | record `cache_read_input_tokens` / `cached_tokens`; a silent drop to 0% means something above the tail changed |

---

## 14. Migration

0. **Level-aware host surfaces** — the §7.1 sandbox fix (becomes the L2 surface), the
   `splash-native` `with_instruction_limit` one-liner, runtime-bound event identity (§7.2).
1. **Message-per-segment wire form** (§9.1) with cache telemetry.
2. **The L0 grammar in `splash-core`** — parser, validator, components with declared local
   state, lowering to `UiNode`, empty host surface.
3. **Port the plan layer onto it.** `app/app/src/app/plan/` already has correct L0 *semantics*;
   give it the L0 grammar plus segment/fold/overlay rather than replacing it.
4. **`splash-app-catalog`.**
5. **Grow `splash-render`; define `splash-backend`; declarative `MapView`; conformance.**
6. **Backend parity** — OH sheds its layers; Android is built.
7. **`splash-ledger`** with level and instance-state enforcement — after §8.2 is resolved.
8. **Reconciliation (§6), then L1, then containment, then L2 idle refinement.**

Steps 0–4 deliver three of four domains with no containment problem.

---

## 15. Risks

| # | Risk | Status |
|---|---|---|
| Q1 | Cards hold ambient `fs`/`run`/`net`/`quit` | **live — fix scoped (§7.1)** |
| Q2 | Per-card isolation is a string replace | **live — §7.2** |
| Q3 | No LLM-safe UI path in Splash | blocking — L0 is the small first increment |
| Q4 | Ledger storage layout undesigned (§8.2 fork) | **blocking for persistence** |
| Q5 | No mobile execution boundary | open — gates L2 |
| Q6 | No mobile rollback anchor | open |
| Q7 | Literal facts undecidable above L0 | confined to L1/L2 |
| Q8 | Append-time cycle rejection needs declared supersets | medium; L0 unaffected |
| Q9 | Android parity is construction | high cost, low risk |
| Q10 | Multi-device sync undesigned | open |
| Q11 | L0 proves too weak for real cards | **open — R7's "improvement" rested on undercounts; `fn tick()` keeps nav at L2 (§4.5)** |
| Q12 | Instance state across ledger edits | undesigned (§13); safe initial policy is reset-on-version-change |
| Q13 | React-shaped syntax creates false familiarity | open — and broader than hooks: lifecycle, identity, expression placement, event semantics, styling |
| Q14 | **No normative L0 grammar or transitive level classifier** | **blocking for any L0 claim (§4.4)** |
| Q15 | **Component contracts absent** — props, outputs, children, shared state, per-component sources, mount/unmount | **blocking for the component model (§4.3)** |
| Q16 | **Reconciliation protocol absent** — prop equality, invalidation, cancellation, focus/scroll preservation, command-vs-property semantics | **blocking for §4.5 and §6** |
| Q17 | **Cache results are single-run, one prompt shape** | measurements demonstrate mechanism, not hit rate under load (§9) |
| ~~Q18~~ | ~~Cache benefit unrealizable~~ | **RESOLVED — mechanism measured** |

---

## 16. How we would know it works

| # | Claim | Test | Result |
|---|---|---|---|
| T0 | Ambient APIs gone at L2 | card calls `mod.fs.read_to_string` | pending |
| T1 | Cards still render | weather/news/stock/nav on the 6T | pending |
| T2 | Native path bounded | `while true {}` in splash-native | pending |
| T3 | **Append preserves cache** | append a segment | **✅ 96.9–99.0%, three providers** |
| T4 | **Mutation destroys it** | edit one word mid-document | **✅ 0%** |
| T5 | **Shared baseline serves many users** | same baseline, different requests | **✅ 99.5%** |
| T6 | **Cache is prefill-only** | TTFT vs decode | **✅ −50% short output** |
| T7 | **Splash beats JSON on emission** | same card both ways | **✅ 262 vs 577 tokens** |
| T8 | L0 suffices | rebuild weather/news/stock as L0 ledgers | pending — tests Q11 |
| T9 | **Components compose** | 7 forecast rows, independent local state | pending |
| T10 | **Instance state survives an edit** | overlay redefines a component; check live instances | pending — tests Q12 |
| T11 | L0 facts are decidable | literal in a data position | pending |
| T12 | Reconciliation bounded | toggle one row; count re-realized instances | pending |
| T13 | Fold deterministic | fold the same chain 10× | pending |
| T14 | Backends agree | conformance corpus on all three | pending |

---

## 17. Summary

The ledger is an append-only segment chain, per app, authoritative for the app's generated
declaration layer. **Its lifecycle and its grammar are separate decisions.**

**Two things are demonstrated. The rest is proposal.**

Demonstrated:

1. **A stable prefix caches and mutation destroys it** — 96.9–99.0% versus 0%, three
   providers, single runs, one prompt shape. Enough to establish the mechanism and the wire
   rule: one message per sealed segment, appends never grow an earlier one.
2. **Restricted Splash is 2.20× denser than equivalent JSON** for the same card, and the
   saving falls on output, which no cache touches.

Proposed, and not yet specified enough to build:

3. **The ledger declares shapes; the runtime owns values** — including component-local state.
   Instance identity, props, outputs, children, lifecycle and state versioning are all open.
4. **Grammar levels** — L0's intended property is *authority confinement*, which is narrower
   than the "decidable" claimed in R5–R7 and does not include termination or bounded state.
   Evaluator resource containment remains mandatory at every level.
5. **No facts** — decidable only for literals in typed data positions.
6. **Declared dependencies** — declaration is the contract; tracking is the optimization.

Two items are urgent regardless of any of this: **cards can read and write arbitrary files and
kill the app today** (§7.1), and per-card isolation is a string replace (§7.2).

Four remain unresolved enough to block building on them: **the storage layout** (§8.2), **the
mobile rollback anchor** (§8.3), **instance state evolution** (§13), and **a normative L0
grammar with a transitive level classifier** (§4.4).

**Q11 is not improved.** R7 claimed nav's L2 requirement was purely reconciliation, using
undercounted evidence. Corrected: `fn tick()` carries mutable route assembly, arithmetic and
source polling, so nav stays L2 even with a declarative `MapView`. Reconciliation is necessary,
not sufficient.
