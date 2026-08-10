#!/usr/bin/env bash
# Build the Android APK *with* the phone-local octos kernel bundled.
#
# The kernel is not an optional extra: with `liboctos.so` in the APK the app
# execs it and talks to it over stdio, so the phone needs no `octos serve`, no
# reverse tunnel and no HTTP auth. Without it the app silently falls back to a
# remote WebSocket, which looks identical at boot and then fails offline.
#
# The kernel must come from the PINNED `octos` submodule. The first working
# build was made from a side clone at a different commit, so the APK shipped a
# kernel that nothing in this tree recorded — that is the hole this script
# closes. It builds the submodule, then hands the binary to `cargo makepad`
# through MAKEPAD_ANDROID_EXTRA_LIBS.
#
#   tools/build-android.sh            # build the APK
#   tools/build-android.sh run        # build, install and launch
#   SKIP_KERNEL=1 tools/build-android.sh   # reuse an already-built kernel
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ACTION="${1:-build}"
PROFILE=release
TARGET=aarch64-linux-android
# API 33 to match cargo-makepad's own toolchain; the `NN-clang` wrapper name
# encodes it, so this number and the wrapper below must agree.
API=33
KERNEL_FEATURES="${KERNEL_FEATURES:-api,git,ast}"
KERNEL_BIN="$ROOT/octos/target/$TARGET/$PROFILE/octos"

# ── the NDK ───────────────────────────────────────────────────────────────────
# It ships with cargo-makepad, not with the Android SDK, under a host-specific
# directory. Discover it rather than hardcoding: the earlier setup pinned an
# absolute path into an uncommitted .cargo/config.toml, which is exactly what
# made the build unreproducible on any other machine.
NDK_ROOT="$(find "$ROOT/makepad/tools/cargo_makepad" -maxdepth 3 -type d -name ndk 2>/dev/null | head -1)"
if [[ -z "$NDK_ROOT" ]]; then
  echo "error: no NDK under makepad/tools/cargo_makepad — run 'cargo makepad android install-toolchain'" >&2
  exit 1
fi
NDK="$(find "$NDK_ROOT" -maxdepth 1 -mindepth 1 -type d | sort -V | tail -1)"
TOOLCHAIN="$(find "$NDK/toolchains/llvm/prebuilt" -maxdepth 1 -mindepth 1 -type d | head -1)/bin"
[[ -d "$TOOLCHAIN" ]] || { echo "error: no LLVM prebuilt toolchain in $NDK" >&2; exit 1; }

if [[ "${SKIP_KERNEL:-0}" != 1 ]]; then
  echo "==> kernel: $(git -C "$ROOT/octos" rev-parse --short HEAD) via $TOOLCHAIN"

  # Two sets of variables, because two different consumers need telling:
  #
  #   CARGO_TARGET_*_LINKER / _AR — rustc's own link step.
  #   CC_/CXX_/AR_/RANLIB_<target> — cc-rs, for the C dependencies. Without
  #   these it looks for an UNVERSIONED `aarch64-linux-android-clang`, which
  #   the NDK stopped shipping; the build then fails deep inside a -sys crate
  #   with "no such file or directory" rather than anything about Android.
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN/${TARGET}${API}-clang"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="$TOOLCHAIN/llvm-ar"
  export CC_aarch64_linux_android="$TOOLCHAIN/${TARGET}${API}-clang"
  export CXX_aarch64_linux_android="$TOOLCHAIN/${TARGET}${API}-clang++"
  export AR_aarch64_linux_android="$TOOLCHAIN/llvm-ar"
  export RANLIB_aarch64_linux_android="$TOOLCHAIN/llvm-ranlib"

  ( cd "$ROOT/octos" && cargo build --"$PROFILE" --target "$TARGET" \
      -p octos-cli --bin octos --features "$KERNEL_FEATURES" )
fi

[[ -f "$KERNEL_BIN" ]] || { echo "error: no kernel at $KERNEL_BIN" >&2; exit 1; }
echo "==> kernel binary: $(du -h "$KERNEL_BIN" | cut -f1) $KERNEL_BIN"

# ── the APK ───────────────────────────────────────────────────────────────────
# `liboctos.so` is the name Android will extract into nativeLibraryDir, and the
# only place an untrusted_app may exec from (W^X: a copy staged anywhere
# app-writable dies with `avc: denied { execute_no_trans }`). The app looks for
# exactly this name — see `find_embedded_kernel` in app/app/src/main.rs.
export MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=$KERNEL_BIN"
# The PGO profdata rustflag in makepad's config is a path relative to the repo
# root, so it has to be made absolute from app/'s cwd.
export RUSTFLAGS="${RUSTFLAGS:--Cprofile-use=$ROOT/aichat/libs/box3d/box3d.profdata}"

cd "$ROOT/app"
exec cargo makepad android "$ACTION" -p octos-app --release
