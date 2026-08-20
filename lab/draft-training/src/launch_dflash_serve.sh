#!/bin/bash
# Serve the TRAINED DFLASH draft on :30880 for an end-to-end tok/s comparison
# against the NGRAM production container on :30878. Same backend flags as
# launch_qwen_ab.sh; only the speculation stack differs.
set -e
DRAFT="${SDRAFT:-/home/ubuntu/qwen38-h200/models/Qwen3.8-27B-DFlash-trained}"
sudo docker rm -f qwen-dflash >/dev/null 2>&1 || true
sudo docker run -d --name qwen-dflash --gpus all --ipc=host --network=host \
  --ulimit memlock=-1 --cap-add SYS_NICE \
  --env HF_HUB_OFFLINE=1 --env TRANSFORMERS_OFFLINE=1 \
  -v /home/ubuntu/qwen38-h200/models/Qwen3.8-27B-FP8-017b9c7:/models/target:ro \
  -v "$DRAFT":/models/draft:ro \
  ominix-sglang-hopper:0.5.16-qwen38 \
  python3 -m sglang.launch_server \
    --model-path /models/target --served-model-name Qwen3.8-27B-FP8-DFlash-trained \
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
    --chunked-prefill-size 32768 --max-prefill-tokens 32768 \
    --default-chat-template-kwargs '{"enable_thinking": false}' \
    --log-level info --watchdog-timeout 3600 \
    --speculative-algorithm DFLASH \
    --speculative-draft-model-path /models/draft \
    --speculative-num-draft-tokens "${SBLOCK:-16}" \
    --speculative-draft-window-size "${SWINDOW:-4096}" \
    --speculative-draft-model-quantization unquant
echo "launched qwen-dflash on :${SPORT:-30880}"
