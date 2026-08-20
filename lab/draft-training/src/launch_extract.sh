#!/bin/bash
# Launch the INSTRUMENTED DFLASH extraction server on port 30879.
# Mirrors launch_qwen_ab.sh's backend flags (so the target numerics match
# production) but swaps NGRAM speculation for DFLASH and bind-mounts the
# instrumented dflash worker over sglang's copy.
#
# This does NOT touch the production container. It only needs HBM headroom.
set -e
REPO=/home/ubuntu/draft-training
DUMP="${DUMP:-/mnt/dflash-dump}"
MEM="${XMEM:-0.42}"
BLOCK="${XBLOCK:-16}"
PORT="${XPORT:-30879}"

test -f "$REPO/src/instrument/dflash_worker_v2.patched.py" || {
  echo "missing patched worker; run patch_worker.py first"; exit 1; }
sudo mkdir -p "$DUMP" && sudo chmod 777 "$DUMP"
echo 0 | sudo tee "$DUMP/floor.txt" >/dev/null

sudo docker rm -f qwen-extract >/dev/null 2>&1 || true
sudo docker run -d --name qwen-extract --gpus all --ipc=host --network=host \
  --ulimit memlock=-1 --cap-add SYS_NICE \
  --env HF_HUB_OFFLINE=1 --env TRANSFORMERS_OFFLINE=1 \
  --env DFLASH_DUMP_DIR=/dump \
  --env DFLASH_DUMP_STEPS="${XSTEPS:-0}" \
  -v /home/ubuntu/qwen38-h200/models/Qwen3.8-27B-FP8-017b9c7:/models/target:ro \
  -v /home/ubuntu/qwen38-h200/models/Qwen3.8-27B-DFlash-0e6412a:/models/draft:ro \
  -v /home/ubuntu/qwen38-h200:/home/ubuntu/qwen38-h200:ro \
  -v "$REPO":/repo:ro \
  -v "$DUMP":/dump \
  -v "$REPO/src/instrument/dflash_worker_v2.patched.py":/sgl-workspace/sglang/python/sglang/srt/speculative/dflash_worker_v2.py:ro \
  ominix-sglang-hopper:0.5.16-qwen38 \
  python3 -m sglang.launch_server \
    --model-path /models/target --served-model-name Qwen3.8-27B-FP8-DFlash-extract \
    --host 127.0.0.1 --port "$PORT" --trust-remote-code --load-format safetensors \
    --quantization fp8 --dtype bfloat16 --tp-size 1 \
    --attention-backend flashinfer --fp8-gemm-backend flashinfer_deepgemm \
    --kv-cache-dtype fp8_e4m3 \
    --max-running-requests "${XMRR:-2}" --cuda-graph-max-bs-decode "${XMRR:-2}" \
    --disable-prefill-cuda-graph \
    --linear-attn-decode-backend triton --linear-attn-prefill-backend flashinfer \
    --mamba-radix-cache-strategy "${XSTRAT:-extra_buffer}" --mamba-ssm-dtype float32 \
    ${XMAMBA:+--max-mamba-cache-size $XMAMBA} \
    --language-only --context-length "${XCTX:-65536}" --mem-fraction-static "$MEM" \
    --chunked-prefill-size "${XCHUNK:-8192}" --max-prefill-tokens "${XCHUNK:-8192}" \
    ${XNOGRAPH:+--disable-cuda-graph} \
    --default-chat-template-kwargs '{"enable_thinking": false}' \
    --log-level info --watchdog-timeout 3600 \
    --speculative-algorithm DFLASH \
    --speculative-draft-model-path /models/draft \
    --speculative-num-draft-tokens "$BLOCK" \
    --speculative-draft-model-quantization "${XDQUANT:-unquant}" \
    ${XWINDOW:+--speculative-draft-window-size $XWINDOW}
echo "launched qwen-extract on :$PORT (mem=$MEM block=$BLOCK dump=$DUMP steps=${XSTEPS:-0})"
