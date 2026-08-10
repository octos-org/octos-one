# octos-splash-native

octos-one's LLM-generated Splash cards, rendered as **native Android views**, with no
makepad renderer in the path.

```
octos-one LLM  ─►  plan  ─►  Splash DSL card     already shipping in octos-one
                                   │
                                   ▼
                        splash-core VM           ymote/Splash — the vendored,
                        (the language only)      renderer-free makepad-script
                                   │
                                   ▼
                           node tree             {kind, attrs, children}
                                   │
                                   ▼  ONE JNI crossing, one flat buffer
                        android.widget.*         ymote/Splash-Android's design:
                                                 Java owns every View
```

## What it is testing

Whether the DSL is genuinely renderer-independent. The card is not rewritten, adapted or
transcribed for this app — it is read from
`/data/data/dev.makepad.octos_app/files/a2app_cards/` when reachable, so the bytes the
model produced are the bytes evaluated here.

`cargo tree` is the check that matters: `makepad-script` and its support crates are
present, `makepad-widgets` / `makepad-draw` / `makepad-platform` are not. If a renderer
crate ever appears there, this app has stopped answering the question it exists for.

## Build

```
cd rust && cargo build --release --target aarch64-linux-android   # needs an NDK
cp target/aarch64-linux-android/release/liboctos_splash_native.so ../app/src/main/jniLibs/arm64-v8a/
cd .. && gradle assembleDebug && adb install -r app/build/outputs/apk/debug/app-debug.apk
```

`adb shell am start -n dev.octos.splashnative/.MainActivity --es card news-app` picks a
different saved card.

## What it will not do yet

Interaction. A card's `on_click` writes state and octos-one re-renders the whole body;
that path is the rebuild problem described in `docs/CARD-STATE-IDENTITY.md`, and wiring
it here before that is settled would bake in the wrong answer.

## Result — verified on a OnePlus 6T, 2026-07-30

Kyoto, 27°, **Mainly Clear**, ↑35° ↓26° ≈33°, seven forecast rows with the correct
weekday run, UV 8, humidity 84%. `fetch: 2 request(s), 106 cache hit(s)` — 108 field
reads across the card cost two HTTP requests.

The header on screen is the claim, stated where a screenshot cannot lose it:
`VM: ymote/Splash (splash-core)   render: android.widget.*   makepad: none`.

### Two things it taught us

**The DSL is portable; the VOCABULARY is not.** The first run rendered
"No node tree — the card evaluated but its root has no `t:` type tag". octos-one's card
is makepad dialect (`SolidView{…}`, `TextHero{…}`), which resolves through makepad's
WIDGET REGISTRY; on a backend with no `makepad-widgets` it evaluates to an object with
no type tag. That is not a renderer bug — the two are different vocabularies over one
language, and the plan is what lets one card exist in both. octos-one now emits the
plain-data form too (`plan::nodes::to_plain_splash`).

**A module is not a name.** The second run rendered the whole tree with every value as
the literal text `[Error:WrongValue]` and a fetch counter stuck at zero. Creating the
`sys` module is not enough — it must also be `set_injected_global`, or `sys.foo(...)` is
an unresolved name whose error value gets *rendered* rather than raised. octos-one does
the same at `aichat/widgets/src/splash.rs:1602`; only the second half had been ported.

### Still missing

`weathericon` renders as `[weathericon]` — the builder here handles framework widgets
only, and that node needs the drawn glyph Splash-Android has. Naming an unknown kind on
screen rather than dropping it is deliberate: a silently missing section looks like a
complete card that is quietly wrong.
