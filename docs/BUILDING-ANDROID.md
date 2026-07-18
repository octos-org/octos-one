# Building for Android

How to build the `octos_app.apk` from a fresh clone, deploy it, and run it on a
device. Default ABI is **aarch64** (arm64-v8a).

## 0. Prerequisites

- Rust (stable) + the Android target: `rustup target add aarch64-linux-android`
- `git`, `python3`, and `adb` (on WSL, use the Windows `adb.exe` to reach
  USB-connected phones — see the WSL note at the bottom)
- **The Android SDK/NDK for `cargo-makepad`.** Installed once with
  `cargo makepad android install-toolchain` (downloads the NDK under
  `makepad/tools/cargo_makepad/android_33_linux_x64/`). ⚠️ **This download hits
  `dl.google.com`, which is blocked on some networks (e.g. WSL behind a proxy).**
  If you already have a populated `android_33_linux_x64/`, reuse it — the build
  does not need to re-download.

## 1. Clone layout

`app/` path-depends on `../aichat`, so the framework fork must sit **beside**
`app/` inside the repo:

```bash
git clone https://github.com/octos-org/octos-one.git
cd octos-one

# the framework fork (Splash engine + sys.* helpers) → ./aichat  (== app/../aichat)
git clone -b octos-one-framework https://github.com/octos-org/makepad.git aichat

# the build tool (cargo-makepad + native composer Java) → ./makepad
git clone -b octos-one-buildtool https://github.com/octos-org/makepad.git makepad

# the octos kernel SOURCE → ./octos  (app/ path-deps octos-core = ../octos/crates/octos-core,
# which uses workspace inheritance, so the whole octos workspace must be present —
# not just the liboctos.so binary from step 3)
git clone https://github.com/octos-org/octos.git octos
```

(`aichat/` and `makepad/` are git-ignored by this repo — they're referenced deps,
not vendored. Both live in the [`octos-org/makepad`](https://github.com/octos-org/makepad)
fork.)

## 2. Install `cargo-makepad`

```bash
# The build tool. The PGO profdata rustflag ships as a RELATIVE path and breaks
# from another CWD, so override it with an absolute one for the install:
RUSTFLAGS="-Cprofile-use=$PWD/aichat/libs/box3d/box3d.profdata" \
  cargo install --path makepad/tools/cargo_makepad --force
```

Then, if you don't already have the NDK, `cargo makepad android install-toolchain`
(see the ⚠️ above).

## 3. Provide `liboctos.so` (the bundled kernel)

The APK bundles the octos kernel as `liboctos.so`. Build it for Android from
[`octos-org/octos`](https://github.com/octos-org/octos) (aarch64 target, features
`api,git,ast`) so you have
`…/octos/target/aarch64-linux-android/release/octos`, or reuse an existing one.

## 4. Build the APK

```bash
cd octos-one/app
export MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=/ABS/PATH/TO/octos/target/aarch64-linux-android/release/octos"
cargo makepad android build -p octos-app --release
```

- Look for `Bundled extra native lib: liboctos.so` and `APK Build completed`.
- Output: `app/target/android/makepad-android-apk/octos_app/apk/octos_app.apk`
  (~74 MB). Verify: `unzip -l …/octos_app.apk | grep -E 'liboctos|libmakepad'`.
- Fast type-check without building the whole APK:
  `RUSTFLAGS="-Cprofile-use=$PWD/../aichat/libs/box3d/box3d.profdata" \
   cargo check --target aarch64-linux-android -p makepad-widgets`
  (a host `cargo check` fails on the Linux desktop backend's `wayland-client`; the
  Android target skips it).

## 5. Install + run

```bash
ADB=adb   # or /mnt/c/.../adb.exe on WSL
$ADB install -r app/target/android/makepad-android-apk/octos_app/apk/octos_app.apk

# launch. Extras (all optional):
#   makepad.OCTOS_PROXY  http proxy for the LLM + data fetches (phones w/o direct net)
#   makepad.APP_CONFIG   'base_url|profile|token'  (headless auth provisioning)
#   makepad.AUTO_PROMPT  auto-submit one prompt on boot (for testing)
$ADB shell am start -S -n dev.makepad.octos_app/.MakepadApp \
    --es makepad.OCTOS_PROXY 'http://127.0.0.1:8899' \
    --es makepad.AUTO_PROMPT 'TSLA'
```

Package `dev.makepad.octos_app`, launch activity `.MakepadApp`. On install Android
extracts `liboctos.so` into the app's `nativeLibraryDir`; the app execs it as
`liboctos.so serve --stdio`.

## Deploy the app-card memory

The app agents only generate good cards if the `a2app/` tree is in the app's octos
profile. **octos assembles it itself** — the kernel looks for an `app-cards/` tree
under the profile's memory dir and, when present, concatenates it (framework →
widget helpers → each app's `app.md` + exemplars) into the injected long-term
memory at inject time. No build step, no generated `MEMORY.md` artifact: the
`a2app/` tree IS the source of truth on disk (see
`octos/crates/octos-memory/src/memory_store.rs` → `assemble_app_cards`).

```bash
# push the a2app tree straight into the profile's memory dir as app-cards/
# (needs root / su on the device)
APPHOME=/data/data/dev.makepad.octos_app/files/octos-home
MEMDIR=$APPHOME/.octos/profiles/_main/data/memory
$ADB push a2app /data/local/tmp/app-cards
$ADB shell "su -c 'rm -rf $MEMDIR/app-cards; cp -r /data/local/tmp/app-cards $MEMDIR/app-cards; chown -R 10210:10210 $MEMDIR/app-cards'"
```

**The injection token budget is handled by the app.** octos's built-in
`memory.max_inject_tokens` default is 2500 — far below the ~23k-token 3-app
tree — and an over-budget tree is truncated **silently** at inject time (the
agent then never sees the exemplars and cards come out with empty values). The
budget knob lives in the KERNEL config, `$APPHOME/.config/octos/config.json`
(NOT in `_main.json` — the current octos profile schema has no
`max_inject_tokens` key, so the old sed recipe silently no-ops). On every boot
the app writes `"memory": {"max_inject_tokens": 40000}` into that file when
the key is absent (`ensure_kernel_memory_budget` in `app/src/main.rs`); an
explicit value you set there yourself is respected.

⚠️ If you tune the value manually, keep it above the assembled-tree token
estimate (`wc -c` over `a2app/` ÷ 4, ~23k tokens for the 3-app tree), or octos
truncates the tail (the last app) on injection. Adding an app is a drop-in:
create `a2app/apps/<id>/app.md` (+ exemplars) and re-push — no code edit.

The per-profile LLM provider + key live in `_main.json` (`config.llm` +
`config.env_vars`) — provision that on the device; **never commit keys**.

## WSL / networking notes

- Use the Windows `adb.exe` (e.g. `/mnt/c/Users/<you>/…/platform-tools/adb.exe`)
  to talk to USB phones; the SDK's Linux `adb` can't see them.
- Phones without a direct internet route reach out through a host proxy: run an
  HTTP CONNECT proxy on the host, `adb reverse tcp:8899 tcp:8899`, and pass
  `--es makepad.OCTOS_PROXY 'http://127.0.0.1:8899'`. This routes both the LLM
  (`api.z.ai` / provider) and the card data fetches (open-meteo, Yahoo, HN).
