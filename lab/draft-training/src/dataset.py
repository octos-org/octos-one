#!/usr/bin/env python3
"""Feature store + window builder shared by train_dflash.py and eval_accept.py.

One training/eval item is an ANCHOR: an absolute token position `a` inside a
recorded completion. The draft at that anchor sees
    context : target aux features for positions [a-W, a)
    block   : [t[a], MASK x (B-1)] at positions [a, a+B)
and must predict t[a+1 .. a+B-1].  (CONDITIONING.md §6)
"""
from __future__ import annotations
import json, os, random
from typing import Dict, List, Optional, Tuple

import torch

from dflash_torch import MASK_TOKEN_ID


class FeatureStore:
    """Sequences whose extraction was incomplete are dropped, not repaired.

    A request whose prompt AND completion prefix were already in the server's
    radix cache gets a deep cache hit, so the target never recomputes those
    positions and the dump only covers the divergent tail (`aligned=False`).
    That happens for harvest jobs whose query is literally identical -- the
    `DASHBOARDS` templates that ignore `{c}` were enqueued once per city. Their
    labels are present but their context features are not, so they are unusable.
    Every dropped sequence is a repeat of a query that survives elsewhere in the
    set, so no unique query is lost.
    """

    def __init__(self, feat_dir: str, cache_size: int = 8):
        self.dir = feat_dir
        self.index = json.load(open(os.path.join(feat_dir, "index.json")))
        all_seqs = [e for e in self.index if e["kind"] == "seq"]
        self.seqs = [e for e in all_seqs if e.get("contiguous") and e.get("aligned")]
        self.dropped = [e for e in all_seqs if e not in self.seqs]
        if self.dropped:
            print(f"[featurestore] dropped {len(self.dropped)} incompletely extracted "
                  f"sequences, kept {len(self.seqs)}")
        self.prefix: Dict[str, dict] = {}
        for e in self.index:
            if e["kind"] == "prefix":
                self.prefix[e["mode"]] = torch.load(
                    os.path.join(feat_dir, f"prefix_{e['mode']}.pt"),
                    map_location="cpu", weights_only=True)
        self._cache: Dict[str, dict] = {}
        self._order: List[str] = []
        self.cache_size = cache_size
        self.by_name = {e["name"]: e for e in self.seqs}

    def seq(self, name: str) -> dict:
        d = self._cache.get(name)
        if d is None:
            d = torch.load(os.path.join(self.dir, "seq", f"{name}.pt"),
                           map_location="cpu", weights_only=True)
            self._cache[name] = d
            self._order.append(name)
            while len(self._order) > self.cache_size:
                self._cache.pop(self._order.pop(0), None)
        return d

    # -------------------------------------------------------------- windows
    def context(self, name: str, lo: int, hi: int) -> Tuple[torch.Tensor, torch.Tensor]:
        """Absolute-position slice [lo, hi) of target aux features, stitched from
        the mode's shared prompt prefix and this sequence's own tail."""
        e = self.by_name[name]
        s = self.seq(name)
        p = self.prefix[e["mode"]]
        parts_h, parts_p = [], []
        s0, sn = s["pos0"], s["h"].shape[0]
        p0, pn = p["pos0"], p["h"].shape[0]
        if lo < s0:
            a, b = max(lo, p0), min(hi, s0, p0 + pn)
            if b > a:
                parts_h.append(p["h"][a - p0: b - p0])
                parts_p.append(torch.arange(a, b))
        if hi > s0:
            a, b = max(lo, s0), min(hi, s0 + sn)
            if b > a:
                parts_h.append(s["h"][a - s0: b - s0])
                parts_p.append(torch.arange(a, b))
        if not parts_h:
            raise ValueError(f"{name}: no context available for [{lo},{hi})")
        return torch.cat(parts_h), torch.cat(parts_p)

    def tokens(self, name: str, lo: int, hi: int) -> torch.Tensor:
        e = self.by_name[name]
        s, p = self.seq(name), self.prefix[e["mode"]]
        out = torch.zeros(hi - lo, dtype=torch.long)
        s0, p0 = s["pos0"], p["pos0"]
        for src, o0 in ((p, p0), (s, s0)):
            n = src["ids"].shape[0]
            a, b = max(lo, o0), min(hi, o0 + n)
            if b > a:
                out[a - lo: b - lo] = src["ids"][a - o0: b - o0].long()
        return out

    def span(self, name: str) -> Tuple[int, int]:
        """[first, last) absolute positions this sequence's own features cover."""
        s = self.seq(name)
        return s["pos0"], s["pos0"] + s["h"].shape[0]

    def gen_span(self, name: str) -> Tuple[int, int]:
        """The completion region: [prompt_len, total_len)."""
        e = self.by_name[name]
        return int(e["prompt_len"]), int(e["total_len"])

    def ctx_floor(self, name: str) -> int:
        e = self.by_name[name]
        p = self.prefix[e["mode"]]
        return int(p["pos0"])


def build_batch(store: FeatureStore, name: str, anchors: List[int], W: int, B: int,
                device="cuda", dtype=torch.bfloat16):
    """One forward's worth of work: several anchors from ONE sequence sharing a
    single context tensor. Exact because attention is masked on absolute
    positions and each block only sees context strictly before its own anchor."""
    anchors = sorted(anchors)
    lo = max(store.ctx_floor(name), anchors[0] - W)
    hi = anchors[-1]                      # last anchor's ctx is [a-W, a)
    h, pos = store.context(name, lo, hi)
    h = h.to(device, dtype, non_blocking=True)
    pos = pos.to(device, non_blocking=True)

    ids, bpos, grp, anc, labels, lmask = [], [], [], [], [], []
    for gi, a in enumerate(anchors):
        toks = store.tokens(name, a, a + B)
        blk = torch.full((B,), MASK_TOKEN_ID, dtype=torch.long)
        blk[0] = toks[0]
        ids.append(blk)
        bpos.append(torch.arange(a, a + B))
        grp.append(torch.full((B,), gi))
        anc.append(torch.full((B,), a))
        labels.append(toks)
        m = torch.ones(B, dtype=torch.bool)
        m[0] = False                       # block position 0 is dropped (worker:120)
        lmask.append(m)
    return dict(
        h=h, ctx_pos=pos,
        ids=torch.cat(ids).to(device), blk_pos=torch.cat(bpos).to(device),
        blk_group=torch.cat(grp).to(device), blk_anchor=torch.cat(anc).to(device),
        labels=torch.cat(labels).to(device), loss_mask=torch.cat(lmask).to(device),
        n_blocks=len(anchors), block_size=B,
    )


def sample_anchors(store: FeatureStore, name: str, n: int, B: int, W: int,
                   rng: random.Random, span: int = 1024) -> List[int]:
    """n anchors inside one contiguous span of the completion (so they can share
    a context tensor without blowing the window up)."""
    g0, g1 = store.gen_span(name)
    s0, s1 = store.span(name)
    g0, g1 = max(g0, s0), min(g1, s1)
    last = g1 - B                      # need B real tokens to label
    if last <= g0:
        return []
    width = min(span, last - g0)
    start = rng.randint(g0, last - width) if last - width > g0 else g0
    picks = sorted(rng.sample(range(start, start + max(1, width)),
                              min(n, max(1, width))))
    return picks
