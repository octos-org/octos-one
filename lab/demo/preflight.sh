#!/usr/bin/env bash
# Run this 10 minutes before the demo. It checks every thing that has actually
# broken during development, then leaves the app launched, warm and quiet.
#
#   ./preflight.sh            # GLM-5.3 via Z.ai  (works today, SLOW first call)
#   ./preflight.sh gh200      # GH200 C2Rust      (fast, needs the box running)
#
# Exit non-zero on any blocker, so a red line means do not walk on stage yet.
set -uo pipefail
export PATH=$PATH:~/Library/Android/sdk/platform-tools

ROUTE="${1:-glm}"
case "$ROUTE" in
  glm)   PROV=/data/local/tmp/prov_glm53; LABEL="GLM-5.3 (Z.ai)";        PROBE="" ;;
  gh200) PROV=/data/local/tmp/prov_gh200; LABEL="GH200 C2Rust FP8";      PROBE="http://192.222.58.176:30878/v1/models" ;;
  *) echo "usage: $0 [glm|gh200]"; exit 2 ;;
esac

PKG=dev.makepad.octos_app
ACT=$PKG/dev.makepad.octos_app.MakepadApp
fail=0
ok()   { printf "  \033[32mOK\033[0m   %s\n" "$1"; }
bad()  { printf "  \033[31mFAIL\033[0m %s\n" "$1"; fail=1; }
warn() { printf "  \033[33mWARN\033[0m %s\n" "$1"; }

echo "== octos-one demo pre-flight — route: $LABEL =="

# 1. Device. USB has dropped mid-session more than once.
n=$(adb devices 2>/dev/null | grep -c "	device$")
[ "$n" -ge 1 ] && ok "phone connected ($(adb devices | grep '	device$' | head -1 | cut -f1))" \
               || bad "no phone — reseat the USB cable"

# 2. App installed.
adb shell pm list packages 2>/dev/null | grep -q "$PKG" \
  && ok "app installed" || bad "app not installed"

# 3. The model endpoint. A dead endpoint is what broke generation before the
#    demo: the app had been provisioned for a GH200 that was shut down, and
#    every prompt died after a 61s connect timeout.
if [ -n "$PROBE" ]; then
  curl -s -m 8 "$PROBE" >/dev/null 2>&1 \
    && ok "GH200 reachable" \
    || bad "GH200 NOT reachable — start the box, or re-run with: $0 glm"
else
  adb shell "ping -c1 -W2 api.z.ai >/dev/null 2>&1 && echo up" 2>/dev/null | grep -q up \
    && ok "phone reaches api.z.ai" || bad "phone cannot reach api.z.ai — check WiFi/captive portal"
fi

# 4. Provisioning profile staged on the device.
adb shell "test -f $PROV/.octos/profiles/dspfac.json && echo y" 2>/dev/null | grep -q y \
  && ok "profile staged: $PROV" || bad "profile missing: $PROV"

[ $fail -ne 0 ] && { echo; echo "BLOCKED — fix the FAIL lines above."; exit 1; }

# 5. Launch clean.
#    DEV_GOAL_FILE points at a path that does not exist ON PURPOSE. On Android
#    an unset var falls through to a mission baked into the APK, so the phone
#    self-starts a card-rewriting loop that replaces whatever is on screen
#    every ~90s. An unreadable path makes the mission None and the loop never
#    starts. This is the single most important line in this script.
echo
echo "== launching (dev loop suppressed) =="
adb logcat -c 2>/dev/null
adb shell am force-stop $PKG 2>/dev/null
adb shell am start -n $ACT \
  --es makepad.PROVISION_DIR "$PROV" \
  --es makepad.DEV_GOAL_FILE /data/local/tmp/__no_dev_loop__ >/dev/null 2>&1
sleep 12

adb shell "ps -A 2>/dev/null | grep -q $PKG" && ok "app running" || bad "app did not start"
d=$(adb logcat -d 2>/dev/null | grep -c "devgoal")
[ "$d" -eq 0 ] && ok "dev loop OFF (0 devgoal lines)" || warn "dev loop ACTIVE ($d lines) — it will overwrite the screen"

# 6. Scope errors in the theme kit. A palette that reads a colour declared
#    further down its own file logs "variable not found in scope" on EVERY
#    render, because `let` evaluates at its own line.
s=$(adb logcat -d 2>/dev/null | grep -c "not found in scope")
[ "$s" -eq 0 ] && ok "no theme scope errors" || warn "$s scope errors in the kit"

echo
if [ $fail -eq 0 ]; then
  echo "READY — now do ONE throwaway prompt to warm the model."
  echo "First call pays full prefill (GLM: ~170s cold, ~60s warm). Do not let"
  echo "the customer watch the cold one."
else
  echo "NOT READY."
fi
exit $fail
