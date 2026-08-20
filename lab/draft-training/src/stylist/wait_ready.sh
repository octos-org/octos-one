#!/bin/bash
# Wait for the research server, failing fast if the container died, and print
# the sizing numbers that matter (KV pool must exceed one 53k prompt + output).
PORT="${1:-30880}"; NAME="${2:-qwen-stylist}"; MAX="${3:-180}"
for i in $(seq 1 "$MAX"); do
  if curl -sf "http://127.0.0.1:$PORT/v1/models" >/dev/null 2>&1; then
    echo "$NAME healthy on :$PORT after $((i*10))s"
    curl -s "http://127.0.0.1:$PORT/get_server_info" | python3 -c \
      'import json,sys; d=json.load(sys.stdin); print("  mem_fraction_static",
       d.get("mem_fraction_static"), "kv_tokens", d.get("max_total_num_tokens"),
       "spec", d.get("speculative_algorithm"))'
    exit 0
  fi
  if ! sudo docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
    echo "$NAME container is gone; last logs:"
    sudo docker logs --tail 80 "$NAME" 2>&1 | tail -80; exit 1; fi
  sleep 10
done
echo "$NAME not healthy in $((MAX*10))s:"; sudo docker logs --tail 80 "$NAME" 2>&1 | tail -80
exit 1
