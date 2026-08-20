#!/bin/bash
set -e
REPO=/home/ubuntu/draft-training
DT=/home/ubuntu/qwen38-h200/draft-training
MODELS=/home/ubuntu/qwen38-h200/models
CKPT="${CKPT:-/dt/ckpt_stylist/draft_final.pt}"
OUTDIR="${OUTDIR:-Qwen3.8-27B-DFlash-stylist}"
sudo mkdir -p "$MODELS/$OUTDIR"
sudo docker run --rm --ipc=host \
  -v "$DT":/dt:ro -v "$REPO":/repo:ro \
  -v "$MODELS/Qwen3.8-27B-DFlash-trained":/models/draft:ro \
  -v "$MODELS/$OUTDIR":/out \
  ominix-sglang-hopper:0.5.16-qwen38 \
  python3 /repo/src/export_draft.py --ckpt "$CKPT" --base /models/draft --out /out
sudo chmod -R a+rX "$MODELS/$OUTDIR"
ls -la "$MODELS/$OUTDIR"
