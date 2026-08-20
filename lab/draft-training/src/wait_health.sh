#!/bin/bash
# Wait for an sglang server to come up, failing fast if its container died.
PORT="${1:-30879}"; NAME="${2:-qwen-extract}"; MAX="${3:-240}"
for i in $(seq 1 "$MAX"); do
  if curl -sf "http://127.0.0.1:$PORT/v1/models" >/dev/null 2>&1; then
    echo "$NAME healthy on :$PORT after $((i*10))s"; exit 0; fi
  if ! sudo docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
    echo "$NAME container is gone; last logs:"; sudo docker logs --tail 60 "$NAME" 2>&1 | tail -60; exit 1; fi
  sleep 10
done
echo "$NAME did not become healthy in $((MAX*10))s; last logs:"
sudo docker logs --tail 60 "$NAME" 2>&1 | tail -60
exit 1
