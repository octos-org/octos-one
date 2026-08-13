#!/bin/bash
# The dumb bridge: validate the dev master's card mechanically, ferry findings.
set -u
ADB="$HOME/Library/Android/sdk/platform-tools/adb"
D=bf0a4730
SM=$HOME/home/Splash-Makepad
R=/tmp/round_card.splash
"$ADB" -s $D pull /data/local/tmp/dev_card.splash "$R" >/dev/null 2>&1

F=/tmp/findings.txt
: > "$F"
echo "FINDINGS (mechanical):" >> "$F"

# 1 · headless translate, default state
{ echo 'let st = { route: "siren", dark: 1 }'; echo 'fn sget(k, d) { return d }'; cat "$SM/components/material/screens/kit.splash" "$R"; } > /tmp/rc_default.splash
T1=$(cd "$SM" && cargo run -q -p splash-makepad --example translate -- /tmp/rc_default.splash 2>&1)
ERRS=$(echo "$T1" | grep -oE '"message":"[^"]+"' | sort -u | head -5)
if [ -n "$ERRS" ]; then
  echo "- translate(default state): ERRORS:" >> "$F"; echo "$ERRS" | sed 's/^/    /' >> "$F"
else
  W=$(echo "$T1" | grep -cE 'Label|Button'); L=$(echo "$T1" | wc -l | tr -d ' ')
  echo "- translate(default state): ok, $L lines, $W text/button widgets" >> "$F"
  if echo "$T1" | grep -qi 'recent'; then echo "- default state SHOWS a Recent-order element (must not, before an order)" >> "$F"; else echo "- default state shows no Recent-order element (correct)" >> "$F"; fi
fi

# 2 · headless translate, placed state (pure-DSL seed shim)
{ echo 'let st = { route: "siren", dark: 1 }'; cat <<'SHIM'
fn sget(k, d) {
  if k == "sb_placed" { return 1 }
  if k == "sb_open_capp" { return 0 }
  if k == "sb_qty_cmac" { return 2 }
  if k == "sb_size_cmac" { return 2 }
  if k == "sb_tab" { return 0 }
  return d
}
SHIM
cat "$SM/components/material/screens/kit.splash" "$R"; } > /tmp/rc_placed.splash
T2=$(cd "$SM" && cargo run -q -p splash-makepad --example translate -- /tmp/rc_placed.splash 2>&1)
E2=$(echo "$T2" | grep -oE '"message":"[^"]+"' | sort -u | head -3)
if [ -n "$E2" ]; then
  echo "- translate(placed state sb_placed=1, qty_cmac=2, size venti, Home tab): ERRORS:" >> "$F"; echo "$E2" | sed 's/^/    /' >> "$F"
else
  if echo "$T2" | grep -qiE 'recent'; then echo "- placed state, Home tab: a Recent-order element IS present (correct)" >> "$F"; else echo "- placed state, Home tab: NO Recent-order element found in the lowered output (feature not visible)" >> "$F"; fi
  if echo "$T2" | grep -qiE 'reorder'; then echo "- placed state: a Reorder control is present" >> "$F"; else echo "- placed state: no Reorder control found" >> "$F"; fi
fi

# 3 · phone render, default state
{ cat "$SM/components/material/screens/kit.splash"; echo 'fn N(k, d) { return sget(k, d) }'; cat "$R"; } > /tmp/rc_device.splash
"$ADB" -s $D push /tmp/rc_device.splash /data/local/tmp/flutter_samples.splash >/dev/null 2>&1
"$ADB" -s $D shell am force-stop dev.makepad.flutter_samples
"$ADB" -s $D shell monkey -p dev.makepad.flutter_samples -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
sleep 8
"$ADB" -s $D exec-out screencap -p > /tmp/rc_shot.png 2>/dev/null
INK=$(python3 -c "
from PIL import Image
from collections import Counter
im = Image.open('/tmp/rc_shot.png').convert('RGB')
px = list(im.resize((60,120)).getdata())
bg = Counter(px).most_common(1)[0][0]
ink = sum(1 for p in px if abs(p[0]-bg[0])+abs(p[1]-bg[1])+abs(p[2]-bg[2])>24)/len(px)
print(f'{ink:.3f}')" 2>/dev/null)
echo "- on-device render (default): ink fraction $INK (blank screen would be < 0.02; the previous working build measured ~0.09)" >> "$F"
echo "" >> "$F"
echo "If everything above is correct and the feature is visible in the placed state, say DONE. Otherwise fix what is named and emit the full card again." >> "$F"
cat "$F"
