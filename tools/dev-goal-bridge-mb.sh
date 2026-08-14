#!/bin/bash
# Movie Box bridge: mechanical findings against the mission's own slot contract.
set -u
ADB="$HOME/Library/Android/sdk/platform-tools/adb"
D=bf0a4730
SM=$HOME/home/Splash-Makepad
R=/tmp/mb_card.splash
"$ADB" -s $D pull /data/local/tmp/dev_card.splash "$R" >/dev/null 2>&1
# Mechanical normalization: markdown code fences are not DSL.
python3 -c "
import pathlib
p = pathlib.Path('$R')
lines = [l for l in p.read_text().splitlines() if l.strip() != chr(96)*3 and not l.strip().startswith(chr(96)*3)]
p.write_text(chr(10).join(lines) + chr(10))"
F=/tmp/mb_findings.txt
: > "$F"
echo "FINDINGS (mechanical):" >> "$F"

run_translate() { # $1 = shim body
  { echo 'let st = { route: "mb", dark: 1 }'; echo "fn sget(k, d) { $1 return d }"; echo 'fn N(k, d) { return sget(k, d) }'; cat "$SM/components/material/screens/kit.splash" "$R"; } > /tmp/mb_eval.splash
  (cd "$SM" && cargo run -q -p splash-makepad --example translate -- /tmp/mb_eval.splash 2>/dev/null)
}

# 1 · default: Browse, all 8, no detail
T=$(run_translate "")
E=$(echo "$T" | grep -oE '"message":"[^"]+"' | sort -u | head -5)
if [ -n "$E" ]; then
  echo "- translate(default): SYNTAX ERRORS (line numbers are within YOUR card file, 1-based):" >> "$F"
  (cd "$SM" && cargo run -q -p splash-makepad --example translate -- /tmp/mb_eval.splash 2>&1) | python3 -c "
import sys, json, pathlib
kit_len = len(pathlib.Path('$SM/components/material/screens/kit.splash').read_text().splitlines())
card = pathlib.Path('$R').read_text().splitlines()
seen = set()
for raw in sys.stdin:
    raw = raw.strip()
    if not raw.startswith('{'): continue
    try: j = json.loads(raw)
    except Exception: continue
    m = j.get('message', {})
    msg = m.get('message', '')[:110]
    for sp in m.get('spans', []):
        ln = sp.get('line_start', 0) - 2 - kit_len  # st line + shim line + kit
        if 1 <= ln <= len(card):
            key = (msg, ln)
            if key in seen: continue
            seen.add(key)
            src = card[ln-1].strip()[:100]
            print(f'    line {ln}: {src}')
            print(f'      -> {msg}')
" | head -16 >> "$F"
else
  LC=$(echo "$T" | wc -l | tr -d ' ')
  [ "$LC" -lt 40 ] && echo "- the lowered tree is nearly EMPTY ($LC lines). The contract's rule: a function that returns a LIST of children must be spliced as \\`c: my_body()\\` — placing it INSIDE \\`c: [ ... ]\\` nests a list in a list, and the child is dropped silently." >> "$F"
  N=0; for t in "Blade Runner" "Dune" "Heat" "Fury Road" "Oldboy" "Lost in Translation" "The Whale" "Ratatouille"; do echo "$T" | grep -q "$t" && N=$((N+1)); done
  echo "- default Browse: $N of 8 catalog titles present" >> "$F"
  echo "$T" | grep -q "Your rating" && echo "- default shows the DETAIL rating section (must not; no mb_open is 1)" >> "$F"
fi

# 2 · genre filter: Sci-Fi only
T=$(run_translate 'if k == "mb_genre" { return 1 }')
S=0; for t in "Blade Runner" "Dune"; do echo "$T" | grep -q "$t" && S=$((S+1)); done
X=0; for t in "Heat" "Ratatouille"; do echo "$T" | grep -q "$t" && X=$((X+1)); done
echo "- genre=Sci-Fi: $S of 2 sci-fi titles shown, $X of 2 non-sci-fi leaked (want 2 and 0)" >> "$F"

# 3 · detail for blade
T=$(run_translate 'if k == "mb_open_blade" { return 1 }')
echo "$T" | grep -q "unearths a secret" && D1=yes || D1=no
echo "$T" | grep -q "Your rating" && D2=yes || D2=no
echo "- detail(mb_open_blade=1): blurb shown: $D1; rating section: $D2 (want yes/yes)" >> "$F"

# 4 · watchlist with two entries, one seen
T=$(run_translate 'if k == "mb_tab" { return 1 } if k == "mb_watch_dune" { return 1 } if k == "mb_watch_heat" { return 1 } if k == "mb_seen_heat" { return 1 }')
echo "$T" | grep -qE "Watchlist \(2\)" && W1=yes || W1=no
echo "$T" | grep -q "Dune" && W2=yes || W2=no
echo "$T" | grep -q "Heat" && W3=yes || W3=no
echo "- watchlist(mb_watch dune+heat, seen heat): count '(2)' in headline: $W1; Dune listed: $W2; Heat listed: $W3" >> "$F"

# 5 · empty watchlist
T=$(run_translate 'if k == "mb_tab" { return 1 }')
echo "$T" | grep -qiE "add movies|from Browse|empty" && W4=yes || W4=no
echo "- empty watchlist: invitation caption present: $W4" >> "$F"

# 6 · device render — ONLY when explicitly asked (FINAL=1). Launching the
# render host steals foreground from octos, and a PAUSED octos freezes the
# whole dev loop: measured, the heartbeat jumped 13s -> 1260s on re-resume.
[ "${FINAL:-0}" != "1" ] && { echo "" >> "$F"; echo "When every line above matches the mission, say DONE and emit the final card." >> "$F"; cat "$F"; exit 0; }
{ cat "$SM/components/material/screens/kit.splash"; echo 'fn N(k, d) { return sget(k, d) }'; cat "$R"; } > /tmp/mb_device.splash
"$ADB" -s $D push /tmp/mb_device.splash /data/local/tmp/flutter_samples.splash >/dev/null 2>&1
"$ADB" -s $D shell am force-stop dev.makepad.flutter_samples
"$ADB" -s $D shell monkey -p dev.makepad.flutter_samples -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
sleep 8
"$ADB" -s $D exec-out screencap -p > /tmp/mb_shot.png 2>/dev/null
INK=$(python3 -c "
from PIL import Image
from collections import Counter
im = Image.open('/tmp/mb_shot.png').convert('RGB')
px = list(im.resize((60,120)).getdata())
bg = Counter(px).most_common(1)[0][0]
print(f'{sum(1 for p in px if abs(p[0]-bg[0])+abs(p[1]-bg[1])+abs(p[2]-bg[2])>24)/len(px):.3f}')" 2>/dev/null)
echo "- on-device render (default): ink fraction $INK (a working screen measures ~0.1-0.3; blank < 0.02)" >> "$F"
echo "" >> "$F"
echo "When every line above matches the mission, say DONE and emit the final card." >> "$F"
cat "$F"

# NOTE: after pushing findings, ALWAYS `adb shell touch` the file — adb push
# preserves the HOST mtime, so an unmodified host file never wakes the poll.
