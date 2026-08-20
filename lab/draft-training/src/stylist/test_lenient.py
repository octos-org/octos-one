#!/usr/bin/env python3
"""Unit-test the lenient accept rule against sglang's exact rule.

Runs on CPU inside the serving image (needs torch only). Checks:
  1. top-k with k=1 and tau=0 reproduces compute_dflash_correct_drafts_and_bonus
     exactly on random data -- the lenient path is a strict superset.
  2. accepts are monotone in k and in tau.
  3. protect_eos never lenient-accepts past a slot where the target wants to stop.
  4. the committed sequence is what the worker will emit (out_tokens contract).
"""
import re, sys, types, torch

HERE = __import__("os").path.dirname(__import__("os").path.abspath(__file__))
src = open(__import__("os").path.join(HERE, "patch_lenient.py")).read()
HELPER = re.search(r"HELPER = '''(.*?)'''", src, re.S).group(1)

g = {"torch": torch, "logger": types.SimpleNamespace(
    info=lambda *a, **k: None, warning=lambda *a, **k: None)}
import os
os.environ.setdefault("DFLASH_LENIENT_EOS_IDS", "")
exec(compile(HELPER, "helper", "exec"), g)
_sl_accept = g["_sl_accept"]


def exact_ref(candidates, target_predict):
    bs = candidates.shape[0]
    matches = candidates[:, 1:] == target_predict[:, :-1]
    correct_len = matches.to(torch.int32).cumprod(dim=1).sum(dim=1)
    bonus = target_predict[torch.arange(bs), correct_len]
    return correct_len, bonus.to(torch.int64)


def commit(candidates, accept_len, bonus):
    """Reproduce the worker's out_tokens[:, :commit_len] for one row."""
    bs, B = candidates.shape
    out = torch.empty((bs, B), dtype=torch.int64)
    out[:, : B - 1] = candidates[:, 1:]
    out[:, B - 1] = 0
    out.scatter_(1, accept_len.to(torch.int64)[:, None], bonus[:, None])
    return [out[i, : int(accept_len[i]) + 1].tolist() for i in range(bs)]


def main():
    torch.manual_seed(0)
    bs, B, V = 6, 16, 512
    fails = 0
    for trial in range(200):
        logits = torch.randn(bs * B, V) * 3
        tp = logits.argmax(-1).view(bs, B)
        # candidates that agree with the target on a random prefix
        cand = torch.randint(0, V, (bs, B))
        for i in range(bs):
            n = torch.randint(0, B, (1,)).item()
            cand[i, 1: 1 + n] = tp[i, :n]

        # 1. degenerate lenient == exact
        for cfg in ({"mode": "topk", "k": 1}, {"mode": "tau", "tau": 0.0}):
            a, b = _sl_accept(candidates=cand, next_token_logits=logits,
                              target_predict=tp, cfg=cfg)
            ea, eb = exact_ref(cand, tp)
            if not (torch.equal(a.to(torch.int64), ea.to(torch.int64))
                    and torch.equal(b, eb)):
                print(f"FAIL degenerate {cfg} trial {trial}"); fails += 1
            if commit(cand, a, b) != commit(cand, ea, eb):
                print(f"FAIL commit {cfg} trial {trial}"); fails += 1

        # 2. monotone in k and tau
        prev = None
        for k in (1, 2, 3, 5, 10):
            a, _ = _sl_accept(candidates=cand, next_token_logits=logits,
                              target_predict=tp, cfg={"mode": "topk", "k": k})
            if prev is not None and bool((a < prev).any()):
                print(f"FAIL monotone k={k} trial {trial}"); fails += 1
            prev = a
        prev = None
        for tau in (0.0, 0.5, 1.0, 3.0, 100.0):
            a, _ = _sl_accept(candidates=cand, next_token_logits=logits,
                              target_predict=tp, cfg={"mode": "tau", "tau": tau})
            if prev is not None and bool((a < prev).any()):
                print(f"FAIL monotone tau={tau} trial {trial}"); fails += 1
            prev = a
        # tau=inf accepts everything
        if int(prev.min()) != B - 1:
            print(f"FAIL tau=inf did not accept all ({prev.tolist()}) trial {trial}")
            fails += 1

    # 3. protect_eos
    g["_SL_EOS"] = [7]
    g["_SL_STATE"]["eos"] = None
    tp = torch.full((1, B), 3, dtype=torch.int64)
    tp[0, 2] = 7                      # target wants to stop at slot 2
    cand = torch.full((1, B), 3, dtype=torch.int64)
    cand[0, 0] = 0
    logits = torch.zeros(B, V)        # flat: every token is inside any top-k
    for i in range(B):
        logits[i, tp[0, i]] = 10.0
    a, _ = _sl_accept(candidates=cand, next_token_logits=logits, target_predict=tp,
                      cfg={"mode": "tau", "tau": 100.0, "protect_eos": True})
    if int(a[0]) != 2:
        print(f"FAIL protect_eos: accept_len={int(a[0])}, expected 2"); fails += 1
    a, _ = _sl_accept(candidates=cand, next_token_logits=logits, target_predict=tp,
                      cfg={"mode": "tau", "tau": 100.0, "protect_eos": False})
    if int(a[0]) != B - 1:
        print(f"FAIL protect_eos off: accept_len={int(a[0])}"); fails += 1

    print("FAILURES:", fails) if fails else print("all lenient-accept tests pass")
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
