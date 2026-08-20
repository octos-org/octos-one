#!/bin/bash
# Research server for the stylist experiment: DFLASH with the LENIENT-verify
# worker bind-mounted over sglang's copy. Backend flags are identical to
# launch_qwen_ab.sh / launch_dflash_serve.sh; only the speculation stack and the
# worker file differ. Production defaults are never touched.
#
# The lenient rule is driven by $CTRL/lenient.json, re-read at runtime, so every
# arm of the experiment runs inside ONE launch -- two 27B servers do not fit on
# this card and each launch costs a production outage window.
#
# It is sized to coexist with a SHRUNK production container (gpu_window.sh open
# 0.42, ABMRR=1 ABMAMBA=5) so this experiment costs no production outage. That
# means SMEM is a fraction of what is free at ITS start, not of the card --
# the trap that caused the 9-minute outage on 2026-08-19. Absolute tok/s here is
# therefore NOT comparable to FINDINGS.md's dedicated-card benchmark; only the
# arm-to-arm comparison inside one launch is.
#
#   SDRAFT=<draft model dir>  SPORT=30880  CTRL=/mnt/stylist-ctrl
#   SMEM=0.86 SMRR=1 SCTX=65536 SCHUNK=8192
set -e
REPO=/home/ubuntu/draft-training
DRAFT="${SDRAFT:-/home/ubuntu/qwen38-h200/models/Qwen3.8-27B-DFlash-stylist}"
CTRL="${CTRL:-/mnt/stylist-ctrl}"
WORKER="$REPO/src/stylist/dflash_worker_v2.lenient.py"

test -f "$WORKER" || { echo "missing $WORKER; run patch_lenient.py first"; exit 1; }
test -d "$DRAFT"  || { echo "missing draft dir $DRAFT"; exit 1; }
sudo mkdir -p "$CTRL" && sudo chmod 777 "$CTRL"
[ -f "$CTRL/lenient.json" ] || echo '{"mode":"exact"}' > "$CTRL/lenient.json"

sudo docker rm -f "${SNAME:-qwen-stylist}" >/dev/null 2>&1 || true
sudo docker run -d --name "${SNAME:-qwen-stylist}" --gpus all --ipc=host --network=host \
  --ulimit memlock=-1 --cap-add SYS_NICE \
  --env HF_HUB_OFFLINE=1 --env TRANSFORMERS_OFFLINE=1 \
  --env DFLASH_LENIENT_CTRL=/control/lenient.json \
  --env DFLASH_LENIENT_STATS=/control \
  --env DFLASH_LENIENT_EOS_IDS="248046,248044" \
  -v /home/ubuntu/qwen38-h200/models/Qwen3.8-27B-FP8-017b9c7:/models/target:ro \
  -v "$DRAFT":/models/draft:ro \
  -v /home/ubuntu/qwen38-h200:/home/ubuntu/qwen38-h200:ro \
  -v "$REPO":/repo:ro \
  -v "$CTRL":/control \
  -v "$WORKER":/sgl-workspace/sglang/python/sglang/srt/speculative/dflash_worker_v2.py:ro \
  ominix-sglang-hopper:0.5.16-qwen38 \
  python3 -m sglang.launch_server \
    --model-path /models/target --served-model-name Qwen3.8-27B-FP8-DFlash-stylist \
    --host 127.0.0.1 --port "${SPORT:-30880}" --trust-remote-code --load-format safetensors \
    --quantization fp8 --dtype bfloat16 --tp-size 1 \
    --attention-backend flashinfer --fp8-gemm-backend flashinfer_deepgemm \
    --kv-cache-dtype fp8_e4m3 \
    --max-running-requests "${SMRR:-4}" --cuda-graph-max-bs-decode "${SMRR:-4}" \
    --disable-prefill-cuda-graph \
    --linear-attn-decode-backend triton --linear-attn-prefill-backend flashinfer \
    --mamba-radix-cache-strategy extra_buffer --mamba-ssm-dtype float32 \
    ${SMAMBA:+--max-mamba-cache-size $SMAMBA} \
    --language-only --context-length "${SCTX:-262144}" --mem-fraction-static "${SMEM:-0.75}" \
    --chunked-prefill-size "${SCHUNK:-32768}" --max-prefill-tokens "${SCHUNK:-32768}" \
    --default-chat-template-kwargs '{"enable_thinking": false}' \
    ${SNOGRAPH:+--disable-cuda-graph} \
    ${SDETERM:+--enable-deterministic-inference} \
    --log-level info --watchdog-timeout 3600 \
    --speculative-algorithm DFLASH \
    --speculative-draft-model-path /models/draft \
    --speculative-num-draft-tokens "${SBLOCK:-16}" \
    --speculative-draft-window-size "${SWINDOW:-4096}" \
    --speculative-draft-model-quantization unquant
echo "launched ${SNAME:-qwen-stylist} on :${SPORT:-30880} draft=$DRAFT ctrl=$CTRL"
