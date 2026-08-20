#!/bin/bash
# Distillation follow-up (STYLIST.md section 7): the CARD-draft training run,
# with hard-label cross-entropy replaced by KL onto the target's own soft
# distribution. Every other hyperparameter is copied verbatim from ckpt_long's
# saved args so the ONLY difference from the shipped card draft is the loss.
set -e
REPO=/home/ubuntu/draft-training
DT=/home/ubuntu/qwen38-h200/draft-training
INIT="${INIT:-/home/ubuntu/qwen38-h200/models/Qwen3.8-27B-DFlash-0e6412a}"
OUT="${OUT:-ckpt_kl}"
ALPHA="${ALPHA:-1.0}"
TEMP="${TEMP:-1.0}"
SPLITS="${SPLITS:-/dt/splits}"

sudo docker run --rm --gpus all --ipc=host --network=host \
  --ulimit memlock=-1 --shm-size=16g \
  --env HF_HUB_OFFLINE=1 --env TRANSFORMERS_OFFLINE=1 \
  -v /mnt/dflash-feats:/feats:ro \
  -v /mnt/dflash-teach:/teach:ro \
  -v "$DT":/dt \
  -v "$INIT":/models/draft:ro \
  -v /home/ubuntu/qwen38-h200/models/Qwen3.8-27B-FP8-017b9c7:/models/target:ro \
  -v "$REPO":/repo:ro \
  ominix-sglang-hopper:0.5.16-qwen38 \
  python3 /repo/src/train_dflash.py \
    --feat-dir /feats --draft /models/draft --target /models/target \
    --out "/dt/$OUT" --splits "$SPLITS" \
    --teacher-dir /teach --kl-alpha "$ALPHA" --kl-temp "$TEMP" \
    --block-sizes 8,16,32,48 --window 4096 --kv-fp8 \
    --anchors 32 --anchor-span 1536 --accum 2 \
    --lr "${LR:-1e-4}" --warmup 60 --epochs "${EPOCHS:-8}" \
    --save-every 1162 --log-every 100
