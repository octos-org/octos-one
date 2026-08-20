#!/bin/bash
# Resize the production container so a second (extraction/training) process fits,
# and put it back afterwards. Only touches the documented ABMEM env knob --
# launch_qwen_ab.sh itself is never edited, per CLAUDE.md.
#
#   ./gpu_window.sh open  0.35    # shrink qwen-ab, keep it serving on :30878
#   ./gpu_window.sh close          # restore the 0.75 default
#   ./gpu_window.sh status
set -e
LAUNCH=/home/ubuntu/qwen38-h200/launch_qwen_ab.sh

wait_healthy() {
  for i in $(seq 1 180); do
    if curl -sf http://127.0.0.1:"${1:-30878}"/v1/models >/dev/null 2>&1; then
      echo "  healthy after ${i}0s"; return 0; fi
    sleep 10
  done
  echo "  NOT healthy after 30min"; return 1
}

case "$1" in
  open)
    MEM="${2:-0.35}"
    echo "shrinking qwen-ab to ABMEM=$MEM (was $(curl -s http://127.0.0.1:30878/get_server_info 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("mem_fraction_static"))' 2>/dev/null || echo '?'))"
    ABMEM="$MEM" bash "$LAUNCH"
    wait_healthy 30878
    ;;
  close)
    echo "restoring qwen-ab to the ABMEM default (0.75)"
    bash "$LAUNCH"
    wait_healthy 30878
    ;;
  status)
    nvidia-smi --query-gpu=memory.total,memory.used,memory.free --format=csv
    sudo docker ps --format '{{.Names}}\t{{.Status}}' | sed 's/^/  /'
    curl -s http://127.0.0.1:30878/get_server_info 2>/dev/null | python3 -c \
      'import json,sys; d=json.load(sys.stdin); print("  qwen-ab mem_fraction_static", d.get("mem_fraction_static"), "kv_tokens", d.get("max_total_num_tokens"))' 2>/dev/null || echo "  qwen-ab not answering"
    ;;
  *) echo "usage: $0 {open [mem]|close|status}"; exit 1;;
esac
