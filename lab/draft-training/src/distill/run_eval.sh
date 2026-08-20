#!/bin/bash
# acc@48 gates for a distilled checkpoint, identical settings to the card run.
set -e
REPO=/home/ubuntu/draft-training
DT=/home/ubuntu/qwen38-h200/draft-training
CKPT="${CKPT:-/dt/ckpt_kl/draft_final.pt}"
OUTJ="${OUTJ:-/dt/eval_kl.json}"
BASE="${BASE:-/home/ubuntu/qwen38-h200/models/Qwen3.8-27B-DFlash-0e6412a}"
sudo docker run --rm --gpus all --ipc=host --network=host \
  --ulimit memlock=-1 --shm-size=16g \
  --env HF_HUB_OFFLINE=1 --env TRANSFORMERS_OFFLINE=1 \
  -v /mnt/dflash-feats:/feats:ro -v "$DT":/dt \
  -v "$BASE":/models/draft:ro \
  -v /home/ubuntu/qwen38-h200/models/Qwen3.8-27B-FP8-017b9c7:/models/target:ro \
  -v "$REPO":/repo:ro \
  ominix-sglang-hopper:0.5.16-qwen38 \
  python3 /repo/src/eval_accept.py \
    --feat-dir /feats --draft /models/draft --target /models/target \
    ${CKPT:+--ckpt "$CKPT"} \
    --block-size 16 --window 4096 --kv-fp8 \
    --holdout /dt/splits/holdout_all.json --slices /dt/splits/slices.json \
    --per-seq 3 --max-seqs 300 --seams --out "$OUTJ"
