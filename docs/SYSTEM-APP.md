# Running as Launcher & System Component

octos-one registers itself both as a regular app (app-drawer `LAUNCHER`) and
as a **home app** (`HOME` + `DEFAULT` categories on the main activity), so the
user can pick it as the device launcher. It can also be installed as a
**system app** in `/system/priv-app` (flags `SYSTEM` + `PERSISTENT`).

This doc is the verified recipe (tested on an Android 14 x86_64 emulator with
`adb root`; a physical device needs an unlocked bootloader + root to write to
the system partition).

## 1. How the registration works

`app/app/resources/android/AndroidManifest.xml.template` (picked up by
cargo-makepad's custom-manifest support) declares, on `.MakepadApp`:

```xml
<intent-filter>  <!-- app drawer -->
    <action android:name="android.intent.action.MAIN" />
    <category android:name="android.intent.category.LAUNCHER" />
</intent-filter>
<intent-filter>  <!-- home app -->
    <action android:name="android.intent.action.MAIN" />
    <category android:name="android.intent.category.HOME" />
    <category android:name="android.intent.category.DEFAULT" />
</intent-filter>
```

plus `android:persistent="true"` on `<application>` (honored once the app is
installed as a system app — ActivityManager then keeps the process alive and
restarts it when it dies).

## 2. Set as the launcher (no root needed)

```bash
adb shell cmd role add-role-holder android.app.role.HOME dev.makepad.octos_app
adb shell input keyevent KEYCODE_HOME   # lands on octos-one
# verify
adb shell cmd role get-role-holders android.app.role.HOME
```

Or in UI: Settings → Apps → Default apps → Home app → Octos_app.

## 3. Install as a system app (root)

PackageManager does **not** extract native libraries for apps dropped into
`/system/priv-app`, so a naive `adb push app.apk /system/priv-app/` crashes
with `UnsatisfiedLinkError: libmakepad.so not found`. The system partition
(emulator overlayfs scratch: ~146 MB total, ~100 MB free) is also too small
for the 80 MB+ kernel. The deployment therefore splits three ways:

| Piece | Where | Why |
|---|---|---|
| `OctosOne.apk` (slim, libs stripped out, re-signed with the same debug key) | `/system/priv-app/OctosOne/` | the system app package |
| `libmakepad.so`, `libstd-*.so` (uncompressed) | `/system/priv-app/OctosOne/lib/x86_64/` | PMS resolves `nativeLibraryDir` here (the "legacyNativeLibraryDir") |
| `liboctos.so` (the 80 MB+ kernel) | `/data/user/0/dev.makepad.octos_app/files/octos-home/.bin/` | too big for the system image; the app finds it via the staged-kernel fallback in `stdio_spawn()` (`app/src/main.rs`) |

### Recipe (emulator)

```bash
# one-time: boot the emulator with -writable-system, then
adb root && adb remount && adb reboot        # enable overlayfs
adb root && adb remount                      # re-enable after the reboot

# 1. slim APK: remove bundled libs, re-sign with the SAME debug keystore
cp octos_app.apk slim.apk && zip -d slim.apk "lib/*"
java -jar $SDK/build-tools/33.0.1/lib/apksigner.jar sign \
  --ks makepad/tools/cargo_makepad/debug.keystore \
  --ks-key-alias androiddebugkey --ks-pass pass:android slim.apk

# 2. push package + the two small libs beside it
adb shell mkdir -p /system/priv-app/OctosOne/lib/x86_64
adb push slim.apk /system/priv-app/OctosOne/OctosOne.apk
adb push libmakepad.so libstd-*.so /system/priv-app/OctosOne/lib/x86_64/

# 3. reboot once so PMS registers the package (it wipes a pre-created data
#    dir!), THEN stage the kernel with the app's uid
adb reboot && adb wait-for-device && adb root
APPID=$(adb shell dumpsys package dev.makepad.octos_app | grep -m1 appId= | sed 's/[^0-9]//g')
adb shell mkdir -p /data/user/0/dev.makepad.octos_app/files/octos-home/.bin
adb push liboctos.so /data/user/0/dev.makepad.octos_app/files/octos-home/.bin/
adb shell "chmod 755 /data/user/0/dev.makepad.octos_app/files/octos-home/.bin/liboctos.so; \
           chown -R $APPID:$APPID /data/user/0/dev.makepad.octos_app/files"

# 4. verify
adb shell pm list packages -s | grep octos
adb shell dumpsys package dev.makepad.octos_app | grep flags=
#   flags=[ SYSTEM HAS_CODE PERSISTENT ... ]
adb shell am start -n dev.makepad.octos_app/.MakepadApp
#   logcat shows: stdio: octos=/data/user/0/.../octos-home/.bin/liboctos.so
```

Gotchas learned on the emulator: `adb remount` does not survive reboots
(re-run it every boot before writing `/system`); `adb push` into a read-only
`/system` can report success while the final rename fails — always verify with
`md5sum` after pushing; the two adb binaries on the host (e.g. Homebrew vs SDK
platform-tools) fight over one adbd — use ONE adb (`-adb-path` when launching
the emulator).

On a real product the APK would be baked into the system image at build time
(AOSP `PRODUCT_PACKAGES` with `LOCAL_PREBUILT_JNI_LIBS`), which extracts the
libs into the image — the manual layout above mirrors exactly that.
