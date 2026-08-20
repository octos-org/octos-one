#!/usr/bin/env python3
"""Unit test for eval_accept.k_window_accept -- the fencepost that the Goal-5
gates are stated in. No torch, no GPU.

Covering K tokens costs one verify per committed block, and each verify commits
accept_len + 1 tokens (the +1 being the target's own token, which is never an
accepted draft). So the ceiling is K - (number of verifies) and, at best,
K - ceil(K / block_size).
"""
import math, random, sys, os

src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "eval_accept.py")).read()
ns = {}
exec(src[src.index("def k_window_accept"): src.index("@torch.no_grad()")], ns)
k_window_accept = ns["k_window_accept"]

B = 16
CASES = [
    ([15, 15, 15], 48, 45, "3 perfect blocks cover 48 in 3 verifies"),
    ([15], 16, 15, "1 perfect block covers 16 in 1 verify"),
    ([0] * 48, 48, 0, "no acceptance: 48 verifies, nothing accepted"),
    ([15, 15], 32, 30, "2 perfect blocks -> 32-2"),
    ([7, 7, 7, 7, 7, 7], 48, 42, "8 tokens/verify -> 6 verifies"),
    ([15, 0, 15, 15], 48, 44, "a dud block adds a verify: 48-4"),
    ([0, 15, 15, 15], 48, 44, "same cost whether the dud leads or not"),
    ([3, 3], 48, None, "chain ended before covering K"),
    ([15, 15, 15], 8, 7, "K below one block: 8-1"),
    ([2, 2, 2, 2, 2, 2, 2, 2], 16, 10, "3 tokens/verify: 6 verifies to reach 16"),
]


def main():
    bad = 0
    for hist, K, want, why in CASES:
        got = k_window_accept(hist, K)
        ok = got == want
        bad += not ok
        print(f"  {'ok  ' if ok else 'FAIL'} K={K:3d} -> {got} (want {want})   # {why}")
    rnd = random.Random(0)
    for _ in range(5000):
        h = [rnd.randint(0, B - 1) for _ in range(60)]
        for K in (8, 16, 32, 48):
            v = k_window_accept(h, K)
            if v is None:
                continue
            ceiling = K - math.ceil(K / B)
            assert 0 <= v <= ceiling, (h, K, v, ceiling)
            # exact identity: accepted = K - verifies_used
            tot = used = 0
            for acc in h:
                used += 1
                tot += acc + 1
                if tot >= K:
                    break
            assert v == K - used, (h, K, v, used)
    print("  ok   5000 random histories: accepted == K - verifies_used, "
          "and never above K - ceil(K/B)")
    print("K-window accounting:", "OK" if bad == 0 else f"{bad} FAILURES")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
