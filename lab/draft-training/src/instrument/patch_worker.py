#!/usr/bin/env python3
"""Produce an instrumented copy of sglang's dflash_worker_v2.py.

Two dumps, both env-gated so the patched file is a no-op unless asked:

  DFLASH_DUMP_DIR     -- where to write. Unset => untouched behaviour.
  <dir>/floor.txt     -- re-read every prefill; only positions >= floor are kept.
                         The driver rewrites it between phases so the 53k shared
                         prompt is captured once and skipped thereafter.
  DFLASH_DUMP_STEPS=1 -- also dump per-decode-step draft proposals + accept lens
                         (this is the ground truth `verify_parity.py` checks
                         our pure-torch reimplementation against).

Run:  python3 patch_worker.py <src dflash_worker_v2.py> <dst>
"""
import sys, re

HELPER = '''

# ==== BEGIN draft-training instrumentation (patch_worker.py) ================
import os as _dt_os, json as _dt_json, threading as _dt_threading

_DT_DIR = _dt_os.environ.get("DFLASH_DUMP_DIR")
_DT_STEPS = _dt_os.environ.get("DFLASH_DUMP_STEPS") == "1"
_DT_LOCK = _dt_threading.Lock()
_DT_SEQ = [0]


def _dt_floor():
    if not _DT_DIR:
        return None
    try:
        with open(_dt_os.path.join(_DT_DIR, "floor.txt")) as f:
            return int(f.read().strip())
    except Exception:
        return 0


def _dt_dump_prefill(batch, hidden, positions):
    """hidden: [sum(extend_lens), 25600] aux features; positions: same length."""
    if not _DT_DIR or hidden is None:
        return
    try:
        floor = _dt_floor()
        pos_cpu = positions.to("cpu", torch.int64)
        off = 0
        for i, req in enumerate(batch.reqs):
            n = int(batch.extend_lens[i])
            if n <= 0:
                continue
            sl = slice(off, off + n)
            off += n
            p = pos_cpu[sl]
            keep = (p >= floor).nonzero(as_tuple=True)[0]
            if keep.numel() == 0:
                continue
            lo = int(keep[0].item())
            h = hidden[sl][lo:].to("cpu", torch.bfloat16).clone()
            pp = p[lo:].clone()
            with _DT_LOCK:
                _DT_SEQ[0] += 1
                sq = _DT_SEQ[0]
            torch.save({"rid": str(req.rid), "pos": pp, "h": h},
                       _dt_os.path.join(_DT_DIR, f"pf_{sq:07d}.pt"))
    except Exception as e:  # never break serving because of instrumentation
        logger.warning("dflash dump_prefill failed: %s", e)


def _dt_dump_step(batch, block_ids, draft_tokens, positions, accept_len, target_top1):
    if not _DT_DIR or not _DT_STEPS:
        return
    try:
        rec = {
            "rids": [str(r.rid) for r in batch.reqs],
            "block_ids": block_ids.to("cpu").clone(),
            "draft_tokens": draft_tokens.to("cpu").clone(),
            "positions": positions.to("cpu").clone(),
            "accept_len": accept_len.to("cpu").clone() if accept_len is not None else None,
            "target_top1": target_top1.to("cpu").clone() if target_top1 is not None else None,
        }
        with _DT_LOCK:
            _DT_SEQ[0] += 1
            sq = _DT_SEQ[0]
        torch.save(rec, _dt_os.path.join(_DT_DIR, f"st_{sq:07d}.pt"))
    except Exception as e:
        logger.warning("dflash dump_step failed: %s", e)
# ==== END draft-training instrumentation ===================================
'''

PREFILL_ANCHOR = """            self._append_target_hidden_to_draft_kv_by_loc(
                target_hidden=logits_output.hidden_states,
                cache_loc=batch.out_cache_loc,
                positions=positions,
            )"""

PREFILL_NEW = PREFILL_ANCHOR + """
            _dt_dump_prefill(batch, logits_output.hidden_states, positions)"""

# after the accept kernel has produced accept_len / target_predict
STEP_ANCHOR = """        if self._need_mamba_verify_commit:
            assert seq_lens_pre_verify is not None"""

STEP_NEW = """        _dt_dump_step(
            batch,
            block_ids,
            draft_tokens,
            positions_2d,
            accept_len,
            locals().get("target_predict", None),
        )

        if self._need_mamba_verify_commit:
            assert seq_lens_pre_verify is not None"""


def main(src, dst):
    s = open(src).read()
    for anchor in (PREFILL_ANCHOR, STEP_ANCHOR):
        if s.count(anchor) != 1:
            raise SystemExit(f"anchor not unique ({s.count(anchor)}x):\n{anchor[:120]}")
    s = s.replace(PREFILL_ANCHOR, PREFILL_NEW)
    s = s.replace(STEP_ANCHOR, STEP_NEW)
    # insert the helper right after the module logger is defined
    m = re.search(r"^logger = logging\.getLogger\(__name__\)$", s, re.M)
    if not m:
        raise SystemExit("could not find logger definition")
    s = s[: m.end()] + HELPER + s[m.end():]
    open(dst, "w").write(s)
    print(f"wrote {dst} ({len(s)} bytes)")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
