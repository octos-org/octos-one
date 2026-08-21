#!/bin/bash
# Teacher soft-distribution extraction. Pure inference against the healthy
# production server -- no outage window (same rationale as score_cards.py).
set -e
sudo mkdir -p /mnt/dflash-teach && sudo chmod 777 /mnt/dflash-teach
sudo docker run --rm --network=host --ipc=host \
  --env HF_HUB_OFFLINE=1 --env TRANSFORMERS_OFFLINE=1 \
  -v /home/ubuntu/qwen38-h200/models/Qwen3.8-27B-FP8-017b9c7:/models/target:ro \
  -v /home/ubuntu/qwen38-h200:/home/ubuntu/qwen38-h200:ro \
  -v /mnt/dflash-feats:/feats:ro -v /mnt/dflash-teach:/teach \
  -v /home/ubuntu/draft-training:/repo:ro \
  ominix-sglang-hopper:0.5.16-qwen38 \
  python3 /repo/src/distill/extract_logprobs.py \
    --port "${TPORT:-30878}" --topk "${TK:-64}" --concurrency "${TC:-3}"
