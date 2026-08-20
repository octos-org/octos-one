#!/usr/bin/env python3
"""Pure-PyTorch DFlash draft model, built to match sglang's serve-time
conditioning exactly (see ../CONDITIONING.md).

Why a reimplementation instead of sglang's DFlashDraftModel: sglang's version is
inference-only (`@torch.no_grad` on forward), writes K/V through RadixAttention
into a paged pool, and needs a full ModelRunner/ForwardBatch to run. We need
gradients through `fc`, `hidden_norm` and every layer's `kv_proj`, which are the
parameters that sit in front of the context KV cache.

Parity with sglang is verified token-for-token by `verify_parity.py` against
draft proposals dumped from a real DFLASH sglang server.

Layout of one step (block_size B, context window W):

    ctx  : positions [a-W, a)   KV from target hidden states, no Q
    block: positions [a, a+B)   ids [t[a], MASK, MASK, ...], target embeddings
    out  : block position j predicts token at absolute position a+j, j>=1
"""
from __future__ import annotations

import json
import math
import os
from dataclasses import dataclass
from typing import Optional, Tuple

import torch
import torch.nn as nn
import torch.nn.functional as F

MASK_TOKEN_ID = 248070  # <|audio_start|>, per dflash_config.mask_token_id


# ---------------------------------------------------------------- primitives
class RMSNorm(nn.Module):
    """Matches sglang.srt.layers.layernorm.RMSNorm.forward_native."""

    def __init__(self, hidden_size: int, eps: float = 1e-6):
        super().__init__()
        self.weight = nn.Parameter(torch.ones(hidden_size))
        self.eps = eps

    def forward(self, x: torch.Tensor, residual: Optional[torch.Tensor] = None):
        orig_dtype = x.dtype
        x = x.float()
        if residual is not None:
            x = x + residual.float()
            residual = x.to(orig_dtype)
        var = x.pow(2).mean(-1, keepdim=True)
        x = x * torch.rsqrt(var + self.eps)
        x = (x * self.weight.float()).to(orig_dtype)
        return x if residual is None else (x, residual)


def rope_cos_sin(positions: torch.Tensor, head_dim: int, base: float,
                 dtype: torch.dtype) -> Tuple[torch.Tensor, torch.Tensor]:
    """NeoX-style RoPE tables for the given absolute positions.

    Mirrors sglang get_rope(..., is_neox_style=True) with rotary_dim == head_dim:
    inv_freq over head_dim/2, cos/sin then applied with rotate_half.
    Computed in float32 then cast, as sglang does.
    """
    inv = 1.0 / (base ** (torch.arange(0, head_dim, 2, device=positions.device,
                                       dtype=torch.float32) / head_dim))
    f = positions.float()[:, None] * inv[None, :]          # [N, hd/2]
    return f.cos().to(dtype), f.sin().to(dtype)


def apply_rope(x: torch.Tensor, cos: torch.Tensor, sin: torch.Tensor) -> torch.Tensor:
    """x: [N, H, D] ; cos/sin: [N, D/2]."""
    cos = cos[:, None, :]
    sin = sin[:, None, :]
    x1, x2 = x.chunk(2, dim=-1)
    return torch.cat((x1 * cos - x2 * sin, x2 * cos + x1 * sin), dim=-1)


def _maybe_fp8(x: torch.Tensor, on: bool) -> torch.Tensor:
    """Round-trip through float8_e4m3 to emulate an fp8 KV cache.

    sglang's --kv-cache-dtype is global, so the DFLASH draft's context KV is
    stored in fp8_e4m3 at serve time even though the draft weights are bf16.
    Training in bf16 and serving in fp8 flips the argmax wherever two draft
    candidates are near-tied, which is exactly where acceptance is decided.
    Straight-through: the cast is not differentiable, so gradients pass through.
    """
    if not on:
        return x
    q = x.to(torch.float8_e4m3fn).to(x.dtype)
    return x + (q - x).detach()


def head_rmsnorm(x: torch.Tensor, weight: torch.Tensor, eps: float) -> torch.Tensor:
    """Per-head RMSNorm over the last dim (sglang apply_qk_norm)."""
    orig = x.dtype
    xf = x.float()
    xf = xf * torch.rsqrt(xf.pow(2).mean(-1, keepdim=True) + eps)
    return (xf * weight.float()).to(orig)


# ---------------------------------------------------------------- the model
@dataclass
class DFlashCfg:
    hidden_size: int = 5120
    num_hidden_layers: int = 5
    num_attention_heads: int = 32
    num_key_value_heads: int = 8
    head_dim: int = 128
    intermediate_size: int = 17408
    rms_norm_eps: float = 1e-6
    rope_theta: float = 1e7
    sliding_window: int = 2048
    layer_types: Tuple[str, ...] = ("sliding_attention",) * 4 + ("full_attention",)
    block_size: int = 16
    num_context_features: int = 5
    mask_token_id: int = MASK_TOKEN_ID
    vocab_size: int = 248320
    num_org_vocab: int = 248320  # rows of lm_head used by the draft sampler
    ctx_window: Optional[int] = None  # = --speculative-draft-window-size
    kv_fp8: bool = False              # = --kv-cache-dtype fp8_e4m3 on the draft pool

    @staticmethod
    def from_model_dir(path: str) -> "DFlashCfg":
        c = json.load(open(os.path.join(path, "config.json")))
        df = c.get("dflash_config", {}) or {}
        ids = df.get("target_layer_ids") or []
        rp = c.get("rope_parameters", {}) or {}
        return DFlashCfg(
            hidden_size=c["hidden_size"],
            num_hidden_layers=c["num_hidden_layers"],
            num_attention_heads=c["num_attention_heads"],
            num_key_value_heads=c["num_key_value_heads"],
            head_dim=c.get("head_dim", c["hidden_size"] // c["num_attention_heads"]),
            intermediate_size=c["intermediate_size"],
            rms_norm_eps=c.get("rms_norm_eps", 1e-6),
            rope_theta=float(rp.get("rope_theta", c.get("rope_theta", 1e7))),
            sliding_window=int(c.get("sliding_window", 2048)),
            layer_types=tuple(c["layer_types"]),
            block_size=int(c.get("block_size", 16)),
            num_context_features=len(ids) if ids else c["num_hidden_layers"],
            mask_token_id=int(df.get("mask_token_id", MASK_TOKEN_ID)),
            vocab_size=int(c.get("vocab_size", 248320)),
            num_org_vocab=int(c.get("vocab_size", 248320)),
        )


class DFlashAttention(nn.Module):
    def __init__(self, cfg: DFlashCfg, layer_id: int):
        super().__init__()
        self.cfg = cfg
        self.layer_id = layer_id
        h, hd = cfg.hidden_size, cfg.head_dim
        self.nh, self.nkv = cfg.num_attention_heads, cfg.num_key_value_heads
        self.q_proj = nn.Linear(h, self.nh * hd, bias=False)
        self.k_proj = nn.Linear(h, self.nkv * hd, bias=False)
        self.v_proj = nn.Linear(h, self.nkv * hd, bias=False)
        self.o_proj = nn.Linear(self.nh * hd, h, bias=False)
        self.q_norm = nn.Parameter(torch.ones(hd))
        self.k_norm = nn.Parameter(torch.ones(hd))
        self.scaling = hd ** -0.5
        self.window = cfg.sliding_window if cfg.layer_types[layer_id] == "sliding_attention" else None
        # serve-time --speculative-draft-window-size: a visibility window on the
        # most recent N committed tokens, applied to EVERY layer (worker:607-623).
        # None = full context (sglang default).
        self.ctx_window = cfg.ctx_window
        self.kv_fp8 = cfg.kv_fp8

    # --- context path: K/V only, exactly worker._append_target_hidden_* ---
    def ctx_kv(self, ctx_hidden: torch.Tensor, ctx_pos: torch.Tensor
               ) -> Tuple[torch.Tensor, torch.Tensor]:
        hd = self.cfg.head_dim
        k = self.k_proj(ctx_hidden).view(-1, self.nkv, hd)
        v = self.v_proj(ctx_hidden).view(-1, self.nkv, hd)
        k = head_rmsnorm(k, self.k_norm, self.cfg.rms_norm_eps)
        cos, sin = rope_cos_sin(ctx_pos, hd, self.cfg.rope_theta, k.dtype)
        k = apply_rope(k, cos, sin)
        return _maybe_fp8(k, self.kv_fp8), _maybe_fp8(v, self.kv_fp8)

    # --- block path: full QKV + attention over [ctx ; block] ---
    def forward(self, x: torch.Tensor, blk_pos: torch.Tensor,
                ctx_k: torch.Tensor, ctx_v: torch.Tensor, ctx_pos: torch.Tensor,
                blk_group: torch.Tensor, blk_anchor: torch.Tensor) -> torch.Tensor:
        """x [M,H] block-token states for M = (#anchors * block_size) tokens.

        `blk_group[i]` is which draft block token i belongs to and
        `blk_anchor[i]` is that block's anchor (= the sequence length the draft
        would have seen). Several anchors from one sequence share one context
        tensor; the mask makes each block see exactly the context it would see
        at serve time -- context strictly BEFORE its own anchor -- and none of
        the other blocks. Without the anchor cut a later block would read the
        target's hidden state for the very token it is supposed to predict.
        """
        hd = self.cfg.head_dim
        M = x.shape[0]
        q = self.q_proj(x).view(M, self.nh, hd)
        k = self.k_proj(x).view(M, self.nkv, hd)
        v = self.v_proj(x).view(M, self.nkv, hd)
        q = head_rmsnorm(q, self.q_norm, self.cfg.rms_norm_eps)
        k = head_rmsnorm(k, self.k_norm, self.cfg.rms_norm_eps)
        cos, sin = rope_cos_sin(blk_pos, hd, self.cfg.rope_theta, q.dtype)
        q, k = apply_rope(q, cos, sin), apply_rope(k, cos, sin)

        keys = torch.cat([ctx_k, _maybe_fp8(k, self.kv_fp8)], 0)
        vals = torch.cat([ctx_v, _maybe_fp8(v, self.kv_fp8)], 0)
        kpos = torch.cat([ctx_pos, blk_pos], 0)
        kgrp = torch.cat([torch.full_like(ctx_pos, -1), blk_group], 0)

        rep = self.nh // self.nkv
        keys = keys.repeat_interleave(rep, dim=1)
        vals = vals.repeat_interleave(rep, dim=1)

        delta = blk_pos[:, None] - kpos[None, :]
        allow = delta >= 0
        if self.window is not None:
            allow = allow & (delta < self.window)
        is_ctx = (kgrp[None, :] == -1)
        ctx_ok = kpos[None, :] < blk_anchor[:, None]      # ctx strictly before the anchor
        if self.ctx_window is not None:
            # the draft window is anchored on the anchor (= the sequence length the
            # draft would see), not on the individual block position
            ctx_ok = ctx_ok & (kpos[None, :] >= blk_anchor[:, None] - self.ctx_window)
        allow = allow & torch.where(
            is_ctx, ctx_ok,
            kgrp[None, :] == blk_group[:, None],          # never across blocks
        )
        mask = torch.zeros_like(delta, dtype=q.dtype)
        mask.masked_fill_(~allow, float("-inf"))

        out = F.scaled_dot_product_attention(
            q.transpose(0, 1).unsqueeze(0),
            keys.transpose(0, 1).unsqueeze(0),
            vals.transpose(0, 1).unsqueeze(0),
            attn_mask=mask[None, None], scale=self.scaling)
        out = out.squeeze(0).transpose(0, 1).reshape(M, self.nh * hd)
        return self.o_proj(out)


class DFlashMLP(nn.Module):
    def __init__(self, cfg: DFlashCfg):
        super().__init__()
        self.gate_proj = nn.Linear(cfg.hidden_size, cfg.intermediate_size, bias=False)
        self.up_proj = nn.Linear(cfg.hidden_size, cfg.intermediate_size, bias=False)
        self.down_proj = nn.Linear(cfg.intermediate_size, cfg.hidden_size, bias=False)

    def forward(self, x):
        return self.down_proj(F.silu(self.gate_proj(x)) * self.up_proj(x))


class DFlashLayer(nn.Module):
    def __init__(self, cfg: DFlashCfg, layer_id: int):
        super().__init__()
        self.input_layernorm = RMSNorm(cfg.hidden_size, cfg.rms_norm_eps)
        self.self_attn = DFlashAttention(cfg, layer_id)
        self.post_attention_layernorm = RMSNorm(cfg.hidden_size, cfg.rms_norm_eps)
        self.mlp = DFlashMLP(cfg)

    def forward(self, hidden, residual, blk_pos, ctx_k, ctx_v, ctx_pos,
                blk_group, blk_anchor):
        if residual is None:
            residual = hidden
            hidden = self.input_layernorm(hidden)
        else:
            hidden, residual = self.input_layernorm(hidden, residual)
        attn = self.self_attn(hidden, blk_pos, ctx_k, ctx_v, ctx_pos,
                              blk_group, blk_anchor)
        hidden, residual = self.post_attention_layernorm(attn, residual)
        hidden = self.mlp(hidden)
        return hidden, residual


class DFlashDraft(nn.Module):
    def __init__(self, cfg: DFlashCfg):
        super().__init__()
        self.cfg = cfg
        self.fc = nn.Linear(cfg.num_context_features * cfg.hidden_size,
                            cfg.hidden_size, bias=False)
        self.hidden_norm = RMSNorm(cfg.hidden_size, cfg.rms_norm_eps)
        self.layers = nn.ModuleList([DFlashLayer(cfg, i) for i in range(cfg.num_hidden_layers)])
        self.norm = RMSNorm(cfg.hidden_size, cfg.rms_norm_eps)
        self.grad_ckpt = False

    def project_target_hidden(self, target_hidden: torch.Tensor) -> torch.Tensor:
        return self.hidden_norm(self.fc(target_hidden))

    def forward(self, target_hidden: torch.Tensor, ctx_pos: torch.Tensor,
                block_embeds: torch.Tensor, blk_pos: torch.Tensor,
                blk_group: Optional[torch.Tensor] = None,
                blk_anchor: Optional[torch.Tensor] = None) -> torch.Tensor:
        """target_hidden [T, 5*H] ; ctx_pos [T] ; block_embeds [M, H] ; blk_pos [M]
        -> final hidden states [M, H] (feed to the target lm_head)."""
        if blk_group is None:
            blk_group = torch.zeros_like(blk_pos)
        if blk_anchor is None:
            blk_anchor = torch.full_like(blk_pos, int(blk_pos[0].item()))
        ctx_hidden = self.project_target_hidden(target_hidden)
        hidden, residual = block_embeds, None
        for layer in self.layers:
            ck, cv = layer.self_attn.ctx_kv(ctx_hidden, ctx_pos)
            if self.grad_ckpt and self.training:
                hidden, residual = torch.utils.checkpoint.checkpoint(
                    layer, hidden, residual, blk_pos, ck, cv, ctx_pos,
                    blk_group, blk_anchor, use_reentrant=False)
            else:
                hidden, residual = layer(hidden, residual, blk_pos, ck, cv, ctx_pos,
                                         blk_group, blk_anchor)
        hidden, _ = self.norm(hidden, residual)
        return hidden

    # ------------------------------------------------------------- weights
    def load_dflash_checkpoint(self, path: str) -> Tuple[int, list]:
        """Load 0e6412a-style weights. Checkpoint uses separate q/k/v_proj and
        `self_attn.{q,k}_norm.weight`, which is exactly this module's layout."""
        from safetensors.torch import load_file
        sd = load_file(os.path.join(path, "model.safetensors"))
        params = dict(self.named_parameters())
        mapped, missing = {}, []
        for name, p in params.items():
            key = name
            if key.endswith(".q_norm") or key.endswith(".k_norm"):
                key = key + ".weight"
            if key in sd:
                mapped[name] = sd[key].to(p.dtype)
            else:
                missing.append((name, key))
        if missing:
            raise KeyError(f"unmapped draft params: {missing[:8]} "
                           f"(checkpoint has {len(sd)} tensors)")
        used = set()
        for name in params:
            used.add(name + ".weight" if name.endswith((".q_norm", ".k_norm")) else name)
        unused = sorted(set(sd) - used)
        self.load_state_dict(mapped, strict=True)
        return len(mapped), unused


# ------------------------------------------------------- frozen target parts
class TargetHeads(nn.Module):
    """The two frozen target tensors DFlash borrows: embed_tokens and lm_head."""

    def __init__(self, embed: torch.Tensor, lm_head: torch.Tensor, num_org: int,
                 out_device="cuda"):
        super().__init__()
        self.register_buffer("embed", embed, persistent=False)
        self.register_buffer("lm_head", lm_head, persistent=False)
        self.num_org = num_org
        self.out_device = out_device

    @staticmethod
    def load(target_dir: str, device="cuda", dtype=torch.bfloat16,
             embed_on_cpu: bool = True) -> "TargetHeads":
        """embed_tokens is 2.5GB but only a few hundred rows are touched per
        step, so it stays on the host by default and rows are gathered on demand.
        lm_head has to be resident: every draft slot multiplies against it."""
        from safetensors.torch import safe_open
        p = os.path.join(target_dir, "outside.safetensors")
        with safe_open(p, framework="pt", device="cpu") as f:
            emb = f.get_tensor("model.language_model.embed_tokens.weight")
            lm = f.get_tensor("lm_head.weight")
        return TargetHeads(emb.to("cpu" if embed_on_cpu else device, dtype),
                           lm.to(device, dtype), lm.shape[0], out_device=device)

    def embed_block(self, ids: torch.Tensor) -> torch.Tensor:
        if self.embed.device.type == "cpu":
            return F.embedding(ids.to("cpu"), self.embed).to(self.out_device,
                                                             non_blocking=True)
        return F.embedding(ids.to(self.embed.device), self.embed)

    def logits(self, hidden: torch.Tensor) -> torch.Tensor:
        return hidden.to(self.lm_head.dtype) @ self.lm_head[: self.num_org].T


def build_block(bonus_token: int, block_size: int, anchor_pos: int,
                mask_id: int, device) -> Tuple[torch.Tensor, torch.Tensor]:
    """worker:1514-1521 / the Triton prepare_block kernel."""
    ids = torch.full((block_size,), mask_id, dtype=torch.long, device=device)
    ids[0] = bonus_token
    pos = torch.arange(anchor_pos, anchor_pos + block_size, dtype=torch.long, device=device)
    return ids, pos
