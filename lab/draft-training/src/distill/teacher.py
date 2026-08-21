#!/usr/bin/env python3
"""Store of TARGET soft distributions (top-K logprobs per absolute position),
produced by src/distill/extract_logprobs.py.

One file per sequence; entry i holds the target's distribution over the token at
absolute position pos0+i. Positions outside a file's range come back invalid and
are simply excluded from the KL term (they still contribute cross-entropy).
"""
from __future__ import annotations
import os
from typing import Dict, List, Tuple

import torch


class TeacherStore:
    def __init__(self, teach_dir: str, cache_size: int = 4):
        self.dir = teach_dir
        self.cache_size = cache_size
        self._cache: Dict[str, dict] = {}
        self._order: List[str] = []
        self.have = {f[:-3] for f in os.listdir(teach_dir) if f.endswith(".pt")}
        self.k = None

    def __contains__(self, name: str) -> bool:
        return name in self.have

    def _load(self, name: str) -> dict:
        d = self._cache.get(name)
        if d is None:
            d = torch.load(os.path.join(self.dir, f"{name}.pt"),
                           map_location="cpu", weights_only=True)
            if self.k is None:
                self.k = int(d["top_ids"].shape[1])
            self._cache[name] = d
            self._order.append(name)
            while len(self._order) > self.cache_size:
                self._cache.pop(self._order.pop(0), None)
        return d

    def gather(self, name: str, pos: torch.Tensor, device="cuda"
               ) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
        """pos: absolute positions [m] (cpu or cuda). Returns
        (top_ids [m,K] int64, top_lp [m,K] float32, valid [m] bool) on `device`."""
        d = self._load(name)
        p = pos.to("cpu", torch.int64)
        idx = p - int(d["pos0"])
        n = d["top_ids"].shape[0]
        valid = (idx >= 0) & (idx < n)
        safe = idx.clamp(0, max(0, n - 1))
        ids = d["top_ids"].index_select(0, safe).to(torch.int64)
        lp = d["top_lp"].index_select(0, safe).to(torch.float32)
        return (ids.to(device, non_blocking=True),
                lp.to(device, non_blocking=True),
                valid.to(device, non_blocking=True))
