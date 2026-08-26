#!/usr/bin/env bash
# Put the phone in demo state against the H100, then hand it over.
#
#   ./demo.sh                      # launch clean, ready for typed prompts
#   ./demo.sh "weather in Tokyo"   # launch AND auto-submit one prompt (warm-up)
#
# Two things this handles that a tap on the app icon cannot:
#
# 1. THE DEV LOOP. With MAKEPAD_DEV_GOAL_FILE unset, Android falls through to a
#    mission baked into the APK and self-starts a card-rewriting loop that
#    replaces whatever is on screen every ~60-90s. Only a launch intent can
#    suppress it, by naming a path that does not exist. Tapping the icon starts
#    the loop and it WILL overwrite your card mid-demo.
#
# 2. THE GFW. The phone cannot reach 89.169.125.111 directly (measured: 100%
#    packet loss) while the Mac can. So the LLM goes phone -> adb reverse ->
#    Mac -> ssh tunnel -> H100, and the profile points at 127.0.0.1:30878.
#    The phone must stay USB-tethered to this Mac for the whole demo.
set -uo pipefail
export PATH=$PATH:~/Library/Android/sdk/platform-tools

PKG=dev.makepad.octos_app
ACT=$PKG/dev.makepad.octos_app.MakepadApp
# Direct is the default. --tunnel routes through the Mac instead.
#   direct : phone -> 89.169.125.111 (measured 3% packet loss, ~400ms RTT)
#   tunnel : phone -> adb reverse -> Mac -> ssh -> H100 (0% loss, USB + VPN)
# Each request uploads an 80-127k-token body; 3% loss on that is enough to
# reset the stream, which surfaces as "failed to send streaming request".
MODE=direct
[ "${1:-}" = "--tunnel" ] && { MODE=tunnel; shift; }
if [ "$MODE" = "tunnel" ]; then PROV=/data/local/tmp/prov_h100t; else PROV=/data/local/tmp/prov_h100; fi
KEY=/Users/yuechen/home/tensordock/ottos-one.pem
HOST=octos-one@89.169.125.111
PROMPT="${1:-}"

ok(){   printf "  \033[32mOK\033[0m   %s\n" "$1"; }
warn(){ printf "  \033[33mWARN\033[0m %s\n" "$1"; }
bad(){  printf "  \033[31mFAIL\033[0m %s\n" "$1"; FAIL=1; }
FAIL=0

echo "== octos-one demo — H100 (Qwen3.8-27B FP8) — mode: $MODE =="

adb devices 2>/dev/null | grep -q "	device$" && ok "phone connected" || bad "no phone"

if [ "$MODE" = "tunnel" ]; then
# SSH tunnel: Mac:30878 -> H100:30878. Recreate if missing.
if ! curl -s -m 6 http://127.0.0.1:30878/health >/dev/null 2>&1; then
  pkill -f "ssh -f -N -L 30878" 2>/dev/null
  ssh -f -N -L 30878:127.0.0.1:30878 -i "$KEY" -o StrictHostKeyChecking=no \
      -o ExitOnForwardFailure=yes -o ServerAliveInterval=20 "$HOST" 2>/dev/null
  sleep 3
fi
curl -s -m 6 http://127.0.0.1:30878/health >/dev/null 2>&1 \
  && ok "H100 reachable through the Mac tunnel" \
  || bad "tunnel down — check the box, then: ssh -i $KEY $HOST '~/serve.sh'"

adb reverse tcp:30878 tcp:30878 >/dev/null 2>&1 && ok "adb reverse 30878 (phone -> Mac)" \
  || bad "adb reverse failed"
else
  adb reverse --remove-all 2>/dev/null
  curl -s -m 8 http://89.169.125.111:30878/health >/dev/null 2>&1 \
    && ok "H100 reachable directly (mode: direct)" || bad "H100 unreachable"
  LOSS=$(adb shell "ping -c6 -W2 89.169.125.111 2>&1 | grep -oE '[0-9]+% packet loss'" 2>/dev/null | tr -d '\r')
  case "$LOSS" in
    "0% packet loss") ok "phone->H100 no packet loss" ;;
    "")               warn "phone->H100 loss unknown" ;;
    *)                warn "phone->H100 $LOSS - expect retry failures; rerun with --tunnel" ;;
  esac
fi

adb shell "test -f $PROV/.octos/profiles/dspfac.json && echo y" 2>/dev/null | grep -q y \
  && ok "H100 profile staged" || bad "profile missing at $PROV"

# Live data (weather/stock/quake/satellite) is fetched BY THE PHONE, directly.
# The tunnel does not carry it. No phone internet => cards render as "—°".
adb shell "ping -c1 -W3 www.baidu.com >/dev/null 2>&1 && echo y" 2>/dev/null | grep -q y \
  && ok "phone has internet (live data will populate)" \
  || warn "PHONE HAS NO INTERNET — cards will generate but show empty data (—°). Fix WiFi first."

[ $FAIL -ne 0 ] && { echo; echo "BLOCKED."; exit 1; }

adb shell am force-stop $PKG 2>/dev/null
adb logcat -c 2>/dev/null
# Quote the prompt so the device shell keeps it as ONE argument; unquoted,
# "weather in Tokyo" arrived as just "weather".
if [ -n "$PROMPT" ]; then
  adb shell "am start -n $ACT --es makepad.PROVISION_DIR $PROV --es makepad.DEV_GOAL_FILE /data/local/tmp/__no_dev_loop__ --es makepad.AUTO_PROMPT '$PROMPT'" >/dev/null 2>&1
  echo; echo "  launched, auto-submitting: $PROMPT"
else
  adb shell "am start -n $ACT --es makepad.PROVISION_DIR $PROV --es makepad.DEV_GOAL_FILE /data/local/tmp/__no_dev_loop__" >/dev/null 2>&1
  echo; echo "  launched clean — type prompts on the phone"
fi

sleep 12
[ "$(adb logcat -d 2>/dev/null | grep -c devgoal)" -eq 0 ] && ok "dev loop OFF" \
  || bad "dev loop ACTIVE — it will overwrite the screen"
adb logcat -d 2>/dev/null | grep -q "provision: deployed" && ok "H100 profile deployed" || true

echo
echo "  READY. Expect ~60s per card. Do one warm-up before the customer watches."
