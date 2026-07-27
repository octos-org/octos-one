# Building for OpenHarmony / HarmonyOS

How to build `octos_app.hap` (and `aichat`) for `aarch64-unknown-linux-ohos`,
sign it, and deploy it. Companion to [BUILDING-ANDROID.md](BUILDING-ANDROID.md).

> **Read the [Signing](#5-signing--the-one-real-gate) section before you start.**
> A **commercial HarmonyOS phone will not install** anything you can sign with
> the public toolchain. Everything else here works end to end without a Huawei
> account; that one step does not.

## 0. What you do NOT need

DevEco Studio. The whole chain — `hdc`, the native clang toolchain, `hvigor`,
and the HAP signer — is assembled from public artifacts by
`~/ohos-sdk/setup.sh`. DevEco is a multi-GB, login-walled download and is only
required if you need AGC signing material (see §5) or the emulator.

## 1. Toolchain

| Piece | Source |
|---|---|
| `hdc`, native clang, `hap-sign-tool.jar`, community keys | `L2-SDK-MAC-M1-PUBLIC.tar.gz` from `repo.huaweicloud.com/openharmony/os/<ver>/` |
| `hvigor` (HAP build system) | range-extracted from `commandline-tools-linux-x64-*.zip` — it is pure JS, so the Linux build runs on macOS |
| `node` | Homebrew |
| JDK 17 | `brew install openjdk@17` — hvigor and the signer are Java |
| Rust target | `rustup target add aarch64-unknown-linux-ohos --toolchain nightly` |

Then `. ~/ohos-sdk/env.sh` before any build.

**Preferred: `. ~/ohos-sdk/env611.sh`** if you have
`commandline-tools-mac-arm64-6.1.1.280.zip`. That package is a *native* macOS
arm64 build (no borrowing hvigor from the Linux bundle) and its SDK is **API 24**,
matching a HarmonyOS 6.1 device exactly, versus API 18 from the public
OpenHarmony 5.1.0 SDK. Two gotchas when using it:

- it does **not** ship `hdc` (Huawei bundles that with DevEco) — `env611.sh`
  symlinks in the one from the OpenHarmony SDK;
- everything is quarantined after download, so binaries die with **SIGKILL** and
  confusing "No such file or directory" errors until you run
  `xattr -dr com.apple.quarantine <extracted-dir>`. Also verify the extraction:
  a 99 MB `clang-15` was silently skipped on the first pass here (compare
  `unzip -l` count against `find -type f -o -type l`).

It does **not** change what a device will install — its `OpenHarmony.p12` is the
same `7a5efc3e…` as every other distribution.

Three layout traps, all of which fail confusingly:

- cargo-makepad's **macOS** arm wants `$DEVECO_HOME/tools/node` and
  `$DEVECO_HOME/tools/hvigor`. The `tool/node` + top-level `hvigor/` form is the
  *Linux* arm and yields `failed to get node home`.
- hvigor 5.x rejects the tarball's flat SDK layout with *"The SDK management mode
  has changed"*. It wants `<base>/<api-level>/<component>`, so `env.sh` points
  `OHOS_BASE_SDK_HOME` at a symlink farm (`ohos-base/18/{ets,js,native,…}`).
- Do **not** write `local.properties` with `sdk.dir` pointing at the flat root —
  that re-triggers the same error.

## 2. Source changes this port required

All of them stem from one fact: **OpenHarmony reports `target_os = "linux"`**, so
desktop-Linux code compiles for it unless excluded. The correct guard is
`target_env = "ohos"`, which the codebase already uses in `os/linux/mod.rs`.

In `aichat/` (the framework fork):

| File | Why |
|---|---|
| `platform/src/gl_render_bridge.rs` | the `target_os = "linux"` `impl Cx` uses `self.os.opengl_cx`; OHOS keeps EGL state in `display` instead |
| `platform/src/cx_api.rs` | `can_play_type` called `linux_video_playback`, which `os/linux/mod.rs` compiles out for ohos |
| `platform/build.rs` | emitted `-lxkbcommon`; not in the OHOS sysroot |
| `platform/network/src/backend.rs`, `socket_stream.rs`, `backend/ohos.rs` | the desktop backend links `-lssl`/`-lcrypto`; OHOS ships `libnet_ssl`/`libohcrypto`. Ported from **ZhangHanDong/makepad branch `robrix-ohos`**, which had already solved this — check there first for anything else OHOS-shaped |

In `octos/` (the kernel), two vendored crates via `[patch.crates-io]` → `../vendor/`:

- **`mmap-rs`** (via `hnsw_rs` → `octos-memory`) takes `nix` 0.26 with default
  features, enabling `aio`/`mqueue`/`fs` — none compile against OHOS libc (~51
  errors). It only needs `mman`; its one `flock` reference is a doc comment.
- **`rustyline`** calls `nix::ioctl_read_bad!` with `libc::TIOCGWINSZ`. nix
  hardcodes `ioctl_num_type = c_ulong` on linux-like targets, but OHOS declares
  `ioctl(fd, request: c_int, …)`. Casting the constant does **not** help — the
  macro re-casts it — so the wrapper is hand-written for ohos.

`makepad-example-aichat` additionally needed a `[lib]` target (cargo-makepad
builds a cdylib), an absolute `-Cprofile-use` path (the one in
`aichat/.cargo/config.toml` is relative and does not survive the cross-build),
and an `AgentEvent::TextAuthoritative` match arm. **None of those three were
OHOS-specific — the example did not compile for any target.**

## 3. Build

```bash
. ~/ohos-sdk/env.sh
cd octos-one/app
cargo makepad ohos --deveco-home=$DEVECO_HOME deveco -p octos-app --release
```

`deveco` wipes and regenerates `target/makepad-open-harmony/octos_app/`, so
anything staged by hand must be re-staged after it.

**Editing the ArkTS glue does not rebuild it.** Only `deveco` copies
`makepad/tools/open_harmony/deveco/entry/src/main/ets/` into the generated
project; the `build` subcommand compiles whatever ArkTS is already staged. So a
change to `makepad.ets` or `Index.ets` appears to build successfully while the
device keeps running the old code (the symptom is a Rust-side
`property <name> is not function` from `arkts_obj_ref.rs`). Either re-run
`deveco` and re-apply the signing config, or copy the changed files across:

```bash
T=makepad/tools/open_harmony/deveco/entry/src/main/ets
S=app/target/makepad-open-harmony/octos_app/entry/src/main/ets
cp "$T/makepad/makepad.ets" "$S/makepad/makepad.ets"
cp "$T/pages/Index.ets"     "$S/pages/Index.ets"
```

## 3a. Webview cards (`runhtml`, and the youtube app)

`CxOsOp::*SystemBrowser` is implemented on OHOS by an ArkTS `Web` component
overlaid on the makepad XComponent — the same arrangement Android uses (WebView
over SurfaceView). Rust calls `ArkGlue.webview*` through
`ArkTsObjRef::call_js_with_args`; `ArkArg` exists because `napi_value`s may only
be created on the JS thread, so arguments are marshalled inside
`js_after_work_cb`.

Four things about the ArkTS side are load-bearing and each one fails *silently*:

1. **Controllers must not live in observed state.** A `WebviewController`
   reached through an `@Observed`/`@ObjectLink` proxy accepts `loadData()` and
   then does nothing — no exception, no navigation. Controllers live in a plain
   `Map` in `ArkGlue` (`WebViewCtl`); only geometry lives in the `@Observed`
   `WebViewSpec`.
2. **Do not pass `undefined` for `loadData`'s optional arguments.** ArkWeb
   type-checks them and rejects the whole call with code `401`.
3. **Flush the pending document on `onPageEnd`, not `onControllerAttached`.**
   The controller binds *before* the component's initial `src` navigation
   completes, so a document loaded from `onControllerAttached` is wiped a few
   hundred ms later when the `about:blank` load lands.
4. **`webviewUpdate` must not replace the `@State` array.** It runs once per
   frame; handing ArkUI a new array that often rebuilds the `Web` component and
   resets the page. Geometry is observed via `@ObjectLink` property writes
   instead, and the array is only replaced on add/remove.

Ops are also order-independent: Rust does not guarantee `spawn` arrives first
(a card already `spawned` on the Rust side re-sends only `set_html` after a
widget rebuild), so every ArkGlue op creates its entry on demand.

To exercise it without an LLM:

```bash
# the REAL youtube app — docs/youtube-player-reference.html, the full player
# the youtube agent composes (top bar, sticky player, feed, PiP)
OCTOS_SEED_CARD=youtube cargo makepad ohos --deveco-home=$DEVECO_HOME \
  build -p octos-app --release

# a minimal card instead, for diagnosing the webview substrate itself
OCTOS_SEED_CARD=web   cargo makepad ohos --deveco-home=$DEVECO_HOME \
  build -p octos-app --release
```

Either way the seed waits for the app's live-id resolver and rewrites the
card's `live:1` ids before injecting, joining the card's own `octos.handles`
map (channel name → youtube handle) to `youtube_live_cache`. **Never hardcode a
YouTube live id**: it goes stale within days and does not degrade to anything
watchable — the player reports "此直播录像无法播放" ("this live stream recording
is not available"). Verify a suspect id with:

```bash
curl -s -A "Mozilla/5.0" "https://www.youtube.com/watch?v=<ID>" \
  | grep -o '"playabilityStatus":{"status":"[A-Z]*"'
```

### Both substrates at once (webview + GPU on one screen)

The two card substrates compose: a `runsplash` card renders natively on the GL
surface while a `runhtml` card renders in the ArkTS `Web` overlay, each clipped
to its own rect. `OCTOS_SEED_CARD=split` seeds one message holding both fences
(`docs/webview-cards/split-gpu.splash` + `split-web.html`) and gives a half/half
screen: an animated per-pixel fragment shader on top, a live chromium document
below. The `MKPWEB rect` log line shows the overlay sitting at `y=429 h=344`
rather than covering the surface — that clipping is what makes the split work.

Two layout facts to know:

- Splash card heights are **dp** (the device viewport here is ~849dp), and
  `web_block` is a fixed **1900** — near-fullscreen by design. So a web card
  plus anything else cannot both fit unless the native card is small; the split
  card uses `height: 415`.
- Keep both fences in ONE message. The seed re-injects whenever `CHAT_DATA` is
  empty (self-healing after a session restore wipes it), so a two-message pair
  gets duplicated and interleaved.

### The JS→native bridge (cards calling OHOS components)

A `runhtml` card reaches native code exactly as it does on Android: it calls
`octos.invoke(tool, args)`, the kit forwards it to
`octos_native.invoke(callId, tool, argsJson)`, and Rust's `WebCard` widget
dispatches the tool and resolves the card's promise by evaluating JS back into
the page.

On OHOS `octos_native` is an ArkTS **`javaScriptProxy`** on the `Web` component
(the counterpart of Android's `addJavascriptInterface`), declared in
`Index.ets` with `methodList: ['invoke']` and backed by `OctosNativeBridge`.

**Pass the ids as strings.** `browser_id` is a `LiveId` (u64) and `call_id` an
i64, but every JS number is an f64 — sending them numerically silently mangles
anything past 2^53.

Working tools include `fs.list/read/write/mkdir/exists/remove`, `http.fetch`,
`clipboard.write`, `share`, `notify`, `ping`, and `dialog.open`, which opens the
real **system file picker** (ArkTS `DocumentViewPicker` — the HarmonyOS file
manager, not an in-page control) and returns the picked file's name + contents.
The picker needs no permission of its own: it grants access to whatever the user
selects.

Note `web_card.rs` rejects tools whose async result no backend delivers. That
guard was `!cfg!(target_os = "android")`; OHOS now emits `AndroidDialogResult`
from the picker, so `dialog.open` is allowed there too. `download` still has no
OHOS result path and stays rejected.

Exercise it end to end with `OCTOS_SEED_CARD=bridge`, which seeds
`docs/webview-cards/native-bridge.html` — a button per tool with the raw JSON
result on screen.

## 3b. The composer is native (this is what makes typing work)

The chat composer is **not** a makepad widget on OHOS — it is an ArkTS
`TextInput` overlay, the same design Android uses (an EditText pill floating
over the GL surface). Two independent reasons:

- a full-screen card's `PortalList` swallows taps aimed at a makepad-drawn
  composer, and
- makepad has **no text-input bridge on OHOS at all**, so a makepad `TextInput`
  there can never receive a character regardless of focus.

The platform contract already existed and is deliberately cross-platform: ops
`CxOsOp::{Show,Hide,Expand,Collapse}AndroidComposer`, and the actions
`AndroidComposerSubmit{text}` / `…NewApp` / `…Switch` / `…Expand` posted back via
`Cx::post_action`. The names keep the `Android` prefix on every platform — don't
rename them, the app and both backends share the identifiers.

Two things to know when touching this:

1. **`sync_composer` in the app was `#[cfg(target_os = "android")]`.** OHOS is
   `target_os = "linux"`, so it fell into the makepad-drawn branch and the ops
   were never emitted — a complete Rust + ArkTS overlay still shows nothing.
   It is now `#[cfg(any(target_os = "android", target_env = "ohos"))]`.
2. **Post the action and drain it the same tick** (`Cx::post_action(...)` then
   `self.handle_action_receiver()`). `post_action` alone sits in the global
   channel until the next render loop, which — when the app is idle behind a
   rendered card — may not arrive until the next touch.

The overlay's full-bleed container uses `.hitTestBehavior(HitTestMode.None)` so
only the pill itself is interactive and taps elsewhere still reach the webview
and the makepad surface.

## 4. The bundled kernel

`add_dependencies` only copies `lib<crate>.so` → `entry/libs/arm64-v8a/libmakepad.so`.
There is **no `MAKEPAD_ANDROID_EXTRA_LIBS` equivalent**, so stage the kernel
manually — after `deveco`, before `build`:

```bash
cargo build -p octos-cli --bin octos --features api,git,ast \
    --target aarch64-unknown-linux-ohos --release      # in octos/
llvm-strip octos -o .../entry/libs/arm64-v8a/liboctos.so
```

`app/app/src/main.rs` has an ohos arm for discovering it, mirroring Android's
(`/proc/self/maps` → the bundle lib dir; HOME at
`/data/storage/el2/base/files/octos-home`). It probes with a real
`octos --version` exec and falls back to WebSocket if that fails — **it is not
established that a HAP may exec from its bundle libs dir**, and `stdio.is_some()`
also selects the `_main` profile id, so a kernel that cannot launch would
otherwise misconfigure the remote path. (`/data/local/tmp` is confirmed noexec.)

## 5. Signing — the one real gate

hvigor's own `SignHap` task is unusable headlessly: it requires DevEco-*encrypted*
passwords (≥32 chars) that only the IDE can mint. So leave `signingConfigs`
empty, let hvigor emit `*-unsigned.hap`, and sign separately:

```bash
~/ohos-sdk/sign-setup.sh <device-udid> dev.makepad.octos_app
cargo makepad ohos --deveco-home=$DEVECO_HOME build -p octos-app --release
~/ohos-sdk/sign-hap.sh
```

Note the `openharmony application release` entry inside `OpenHarmony.p12` is
**self-signed**; using it as the leaf fails with *"verify certificate chain
failed"*. The CA-issued leaf with the same key pair is the
`development-certificate` embedded in `UnsgnedDebugProfileTemplate.json`.

### Which devices accept this

| Device | Result |
|---|---|
| OpenHarmony (DAYU200, emulator) | **installs** — trusts the community root |
| Commercial HarmonyOS phone | **rejects** — `code:9568257 fail to verify pkcs7 file` |

Verified on a `SUP-AL90` (HarmonyOS 6.1, API 24). The device's own verifier:

```
HapVerify: hap_cert_verify_openssl_utils.cpp(GetCertsChain:322)
  it do not come from trusted root,
  issuer: C=CN, O=OpenHarmony, OU=OpenHarmony Team, CN=OpenHarmony Application Root CA
BMS: hap files check signature info failed 8519743
```

Meanwhile `hap-sign-tool verify-app` reports **success** on the same file — the
HAP is correctly formed and signed. The phone simply does not carry the
OpenHarmony root. Ruled out, so nobody repeats it: the community keys are
**byte-identical** across SDK 5.1.0, SDK 6.1 and Command Line Tools (p12 sha256
`7a5efc3e…`), so no SDK version helps; `hdc install` fails identically; there is
no `bm install` bypass flag; and `hdc smode` is refused on an undebuggable build.

For a commercial device you need **AGC debug material** — DevEco →
*File > Project Structure > Signing Configs > Automatically generate signature*,
which registers the device UDID in AppGallery Connect and emits `.p12`/`.cer`/`.p7b`.
`sign-hap.sh` consumes it via `OHOS_SIGN_P12`, `OHOS_SIGN_P12_PWD`,
`OHOS_SIGN_ALIAS`, `OHOS_SIGN_KEY_PWD`, `OHOS_SIGN_CERT`, `OHOS_SIGN_PROFILE`.

## 6. Deploy

```bash
~/ohos-sdk/finish.sh          # UDID → re-sign → build → install → launch
OCTOS_PUSH_CONFIG=1 ~/ohos-sdk/finish.sh   # also push the LLM provider config
```

hdc authorization needs a physical **Allow** tap on the device; there is no
headless path. Wireless (`hdc tconn <ip>:<port>`) works, but the port rotates
whenever wireless debugging is toggled.

## 7. Debugging an install failure

`hilog -x` hangs a non-interactive shell, and `-x`/`-z` cannot be combined. Use:

```bash
hdc -t <target> shell "hilog -r; bm install -p <dir>; hilog -z 900 > /data/local/tmp/i.log"
hdc -t <target> shell "grep -iE 'verify|signature|profile|cert' /data/local/tmp/i.log"
```

That is what produced the trust-root message above; the `bm` exit code alone only
gives you `9568257`.
