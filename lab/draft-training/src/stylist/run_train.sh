#!/bin/bash
# Train the STYLIST draft: same pipeline and hyperparameters as the successful
# card run, but initialised from the TRAINED card draft and fed only the
# top-quartile-by-design-score cards plus the whole general slice.
set -e
REPO=/home/ubuntu/draft-training
DT=/home/ubuntu/qwen38-h200/draft-training
INIT="${INIT:-/home/ubuntu/qwen38-h200/models/Qwen3.8-27B-DFlash-trained}"
OUT="${OUT:-ckpt_stylist}"
EPOCHS="${EPOCHS:-8}"

sudo docker run --rm --gpus all --ipc=host --network=host \
  --ulimit memlock=-1 --shm-size=16g \
  --env HF_HUB_OFFLINE=1 --env TRANSFORMERS_OFFLINE=1 \
  -v /mnt/dflash-feats:/feats:ro \
  -v "$DT":/dt \
  -v "$INIT":/models/draft:ro \
  -v /home/ubuntu/qwen38-h200/models/Qwen3.8-27B-FP8-017b9c7:/models/target:ro \
  -v "$REPO":/repo:ro \
  ominix-sglang-hopper:0.5.16-qwen38 \
  python3 /repo/src/train_dflash.py \
    --feat-dir /feats --draft /models/draft --target /models/target \
    --out "/dt/$OUT" \
    --splits /dt/splits_stylist \
    --block-sizes 8,16,32,48 --window 4096 --kv-fp8 --grad-ckpt \
    --anchors 24 --anchor-span 768 --accum 2 \
    --lr "${LR:-5e-5}" --warmup 50 --epochs "$EPOCHS" \
    --save-every 400 --log-every 10
