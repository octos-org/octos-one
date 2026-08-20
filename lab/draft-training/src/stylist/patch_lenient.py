#!/usr/bin/env python3
"""Produce a research copy of sglang's dflash_worker_v2.py with LENIENT verify.

Production DFlash verify is exact-match: a draft token is committed only if it
equals the target's argmax at that slot, which is why a draft can change speed
but never output. This patch adds the one mechanism by which a draft CAN change
output -- accept a draft token when it is merely CLOSE to the target's choice:

  top-k     : the draft token is among the target's top-k tokens at that slot
  tau       : the draft token's logprob is within tau of the target's top-1
              (logprob margin == raw logit margin: the logsumexp cancels)

Both fall back to exact match when disabled, and exact matches are always
accepted, so mode "exact" reproduces stock behaviour bit for bit.

Why the emitted text stays coherent: the target ran its forward over the WHOLE
draft block, so target_predict[i+1] is already conditioned on the draft tokens
at slots <= i. Committing a lenient-accepted draft prefix and then the target's
own token at the first rejection therefore yields a valid target continuation of
the accepted prefix -- the draft steers, the target still writes.

Control is a JSON file re-read at most every 0.25 s (not an env var), so all
arms of the experiment run inside ONE server launch. Two 27B servers do not fit
on this card, and every launch costs a production outage window.

  /control/lenient.json  {"mode":"exact"|"topk"|"tau", "k":2, "tau":1.0,
                          "protect_eos":true, "stats":false, "tag":"c_k2"}

`stats` adds a per-slot CPU sync; leave it off for the timed arms.

Run:  python3 patch_lenient.py <src dflash_worker_v2.py> <dst>
"""
import sys, re

HELPER = '''

# ==== BEGIN stylist lenient-verify patch (patch_lenient.py) ================
import os as _sl_os, json as _sl_json, time as _sl_time, threading as _sl_threading

_SL_CTRL = _sl_os.environ.get("DFLASH_LENIENT_CTRL")
_SL_STATS_DIR = _sl_os.environ.get("DFLASH_LENIENT_STATS", "/control")
_SL_EOS = [int(x) for x in
           (_sl_os.environ.get("DFLASH_LENIENT_EOS_IDS") or "").replace(",", " ").split()
           if x.strip()]
_SL_STATE = {"cfg": None, "t": 0.0, "mtime": None, "eos": None, "acc": {}}
_SL_LOCK = _sl_threading.Lock()


def _sl_config():
    """None => stock exact verify. Re-read at most 4x/second."""
    if not _SL_CTRL:
        return None
    now = _sl_time.monotonic()
    if now - _SL_STATE["t"] < 0.25:
        return _SL_STATE["cfg"]
    _SL_STATE["t"] = now
    try:
        st = _sl_os.stat(_SL_CTRL)
        if _SL_STATE["mtime"] != st.st_mtime:
            _SL_STATE["mtime"] = st.st_mtime
            cfg = _sl_json.load(open(_SL_CTRL))
            mode = str(cfg.get("mode", "exact")).lower()
            _SL_STATE["cfg"] = None if mode == "exact" else cfg
            logger.info("DFLASH lenient config -> %s", cfg)
    except Exception as e:
        logger.warning("lenient ctrl read failed (%s); staying exact", e)
        _SL_STATE["cfg"] = None
    return _SL_STATE["cfg"]


def _sl_eos_tensor(device):
    t = _SL_STATE["eos"]
    if t is None or t.device != device:
        t = torch.tensor(_SL_EOS or [-1], dtype=torch.int64, device=device)
        _SL_STATE["eos"] = t
    return t


def _sl_flush_stats(tag):
    with _SL_LOCK:
        acc = _SL_STATE["acc"].get(tag)
        if not acc:
            return
        try:
            p = _sl_os.path.join(_SL_STATS_DIR, "stats_%s.json" % tag)
            _sl_json.dump(acc, open(p, "w"))
        except Exception as e:
            logger.warning("lenient stats flush failed: %s", e)


def _sl_record(tag, exact_n, topk_n, tau_n, slot_n, margins, top2_margins, ranks):
    with _SL_LOCK:
        a = _SL_STATE["acc"].setdefault(
            tag, {"slots": 0, "exact": 0, "topk_only": 0, "tau_only": 0,
                  "steps": 0, "rejections": 0,
                  "margin_hist": [0] * 24, "top2_hist": [0] * 24,
                  "rank_hist": [0] * 14})
        a["steps"] += 1
        a["slots"] += slot_n
        a["exact"] += exact_n
        a["topk_only"] += topk_n
        a["tau_only"] += tau_n
        a["rejections"] += len(margins)
        for h, src in (("margin_hist", margins), ("top2_hist", top2_margins)):
            for v in src:
                # 0.25-wide bins over [0, 6); everything above lands in the last
                a[h][int(min(23, max(0, v / 0.25)))] += 1
        for r in ranks:
            # bins: rank 2..10 -> 0..8, 11-100 -> 9, 101-1000 -> 10, >1000 -> 11
            if r <= 10:
                a["rank_hist"][max(0, r - 2)] += 1
            elif r <= 100:
                a["rank_hist"][9] += 1
            elif r <= 1000:
                a["rank_hist"][10] += 1
            else:
                a["rank_hist"][11] += 1
        if a["steps"] % 200 == 0:
            try:
                p = _sl_os.path.join(_SL_STATS_DIR, "stats_%s.json" % tag)
                _sl_json.dump(a, open(p, "w"))
            except Exception:
                pass


def _sl_accept(*, candidates, next_token_logits, target_predict, cfg):
    """Lenient replacement for compute_dflash_correct_drafts_and_bonus.

    Returns (accept_len int32 [bs], bonus int64 [bs]) with the same contract:
    accept_len draft tokens from candidates[:, 1:] are committed, then the
    target's own token at slot accept_len.
    """
    bs, B = candidates.shape
    if B < 2:
        return (torch.zeros(bs, dtype=torch.int32, device=candidates.device),
                target_predict[:, 0].to(torch.int64))
    lg = next_token_logits.view(bs, B, -1)[:, :-1, :].float()   # [bs, B-1, V]
    cand = candidates[:, 1:].to(torch.int64)                    # [bs, B-1]
    tgt = target_predict[:, :-1]                                # [bs, B-1]
    exact = cand == tgt
    ok = exact.clone()

    mode = str(cfg.get("mode", "exact")).lower()
    k = int(cfg.get("k", 0) or 0)
    tau = float(cfg.get("tau", 0.0) or 0.0)
    topk_hit = None
    tau_hit = None
    if mode == "topk" and k > 1:
        idx = lg.topk(min(k, lg.shape[-1]), dim=-1).indices        # [bs, B-1, k]
        topk_hit = (idx == cand.unsqueeze(-1)).any(-1)
        ok = ok | topk_hit
    if mode == "tau" and tau > 0.0:
        top1 = lg.max(dim=-1).values
        cand_lg = lg.gather(-1, cand.unsqueeze(-1)).squeeze(-1)
        tau_hit = (top1 - cand_lg) <= tau
        ok = ok | tau_hit

    if cfg.get("protect_eos", True) and _SL_EOS:
        eos = _sl_eos_tensor(candidates.device)
        # where the TARGET wants to stop, demand exact agreement: a lenient
        # accept there would silently delete the stop token and run on
        stopish = (tgt.unsqueeze(-1) == eos).any(-1)
        ok = torch.where(stopish, exact, ok)

    accept_len = ok.to(torch.int32).cumprod(dim=1).sum(dim=1)
    bonus = target_predict[torch.arange(bs, device=target_predict.device),
                           accept_len.to(torch.int64)].to(torch.int64)

    if cfg.get("stats"):
        try:
            # The tau/top-k selection data: at the slot where EXACT verify first
            # rejects, how far was the draft's token from the target's top-1, and
            # what rank did it hold? Measurable while still emitting exact-verify
            # output (run this probe with mode "tau", tau 0.0).
            exact_run = exact.to(torch.int32).cumprod(dim=1)
            j = exact_run.sum(dim=1)                       # first rejected slot
            top1v = lg.max(dim=-1).values
            cand_lgv = lg.gather(-1, cand.unsqueeze(-1)).squeeze(-1)
            top2v = lg.topk(2, dim=-1).values
            rows = torch.arange(bs, device=lg.device)
            live = j < (B - 1)
            margins, ranks = [], []
            if bool(live.any()):
                jj = j.clamp(max=B - 2)
                m = (top1v[rows, jj] - cand_lgv[rows, jj])[live]
                rk = (lg[rows, jj] > cand_lgv[rows, jj].unsqueeze(-1)).sum(-1)[live] + 1
                margins = m.tolist()
                ranks = rk.tolist()
            run = ok.to(torch.int32).cumprod(dim=1).bool()
            e = int((exact & run).sum())
            tk = int(((topk_hit & ~exact & run).sum()) if topk_hit is not None else 0)
            tv = int(((tau_hit & ~exact & run).sum()) if tau_hit is not None else 0)
            t2 = (top2v[..., 0] - top2v[..., 1]).reshape(-1).tolist()
            _sl_record(str(cfg.get("tag", "run")), e, tk, tv, int(run.sum()),
                       margins, t2, ranks)
        except Exception as ex:
            logger.warning("lenient stats failed: %s", ex)
    return accept_len, bonus
# ==== END stylist lenient-verify patch ====================================
'''

ANCHOR = """            target_predict = torch.argmax(logits_output.next_token_logits, dim=-1).view(
                bs, int(self.block_size)
            )
            if self._use_triton_accept_bonus:"""

NEW = """            target_predict = torch.argmax(logits_output.next_token_logits, dim=-1).view(
                bs, int(self.block_size)
            )
            _sl_cfg = _sl_config()
            if _sl_cfg is not None:
                accept_len, bonus = _sl_accept(
                    candidates=candidates,
                    next_token_logits=logits_output.next_token_logits,
                    target_predict=target_predict,
                    cfg=_sl_cfg,
                )
                commit_lens = accept_len.to(torch.int32) + 1  # [bs]
                out_tokens = torch.empty(
                    (bs, int(self.block_size)), dtype=torch.int64, device=device
                )
                if int(self.block_size) > 1:
                    out_tokens[:, : int(self.block_size) - 1].copy_(candidates[:, 1:])
                out_tokens[:, int(self.block_size) - 1].fill_(0)
                out_tokens.scatter_(
                    1, accept_len.to(torch.int64)[:, None], bonus[:, None]
                )
            elif self._use_triton_accept_bonus:"""


def main(src, dst):
    s = open(src).read()
    if s.count(ANCHOR) != 1:
        raise SystemExit(f"anchor not unique ({s.count(ANCHOR)}x)")
    s = s.replace(ANCHOR, NEW)
    m = re.search(r"^logger = logging\.getLogger\(__name__\)$", s, re.M)
    if not m:
        raise SystemExit("could not find logger definition")
    s = s[: m.end()] + HELPER + s[m.end():]
    open(dst, "w").write(s)
    print(f"wrote {dst} ({len(s)} bytes)")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
