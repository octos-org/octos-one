#!/usr/bin/env python3
"""Unit-test the distillation loss against brute force. CPU, no GPU needed."""
import os, sys
import torch
import torch.nn.functional as F

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from train_dflash import kl_term

torch.manual_seed(0)
V, K, N = 5000, 64, 256
ok = True


def check(name, a, b, tol):
    global ok
    d = float((a - b).abs().max())
    good = d <= tol
    ok &= good
    print(f"  {'PASS' if good else 'FAIL'}  {name}: max abs diff {d:.3e} (tol {tol:.0e})")


# --- a peaked teacher, like the real one, and a random student --------------
tgt_logits = torch.randn(N, V) * 2
tgt_logits[torch.arange(N), torch.randint(0, V, (N,))] += 14.0   # near one-hot
tlp_full = F.log_softmax(tgt_logits, -1)
t_lp, t_ids = tlp_full.topk(K, dim=-1)
stu = torch.randn(N, V) * 2

# 1. the lumped tail is a COARSENING of the true distribution, so by the data
#    processing inequality it is a lower bound on the true full-vocab KL, and it
#    becomes exact as the top-K mass approaches 1 (the real teacher's top-64
#    holds a median 0.9999, so this is the regime that matters).
exact = (tlp_full.exp() * (tlp_full - F.log_softmax(stu, -1))).sum(-1)
approx = kl_term(stu, t_ids, t_lp)
slack = float((exact - approx).min())
print(f"  {'PASS' if slack >= -1e-3 else 'FAIL'}  lumped-tail KL <= true KL "
      f"(min slack {slack:.3e}); teacher topK mass here is only "
      f"{float(t_lp.exp().sum(-1).mean()):.3f}")
ok &= slack >= -1e-3

sharp = torch.randn(N, V) * 2
sharp[torch.arange(N), torch.randint(0, V, (N,))] += 30.0     # topK mass ~ 1
slp_full = F.log_softmax(sharp, -1)
s_lp, s_ids = slp_full.topk(K, dim=-1)
exact_s = (slp_full.exp() * (slp_full - F.log_softmax(stu, -1))).sum(-1)
print(f"  (teacher topK mass {float(s_lp.exp().sum(-1).mean()):.6f})")
check("T=1 KL is exact when the tail is empty", kl_term(stu, s_ids, s_lp),
      exact_s, 2e-3)

# 2. zero when the student IS the teacher
check("KL(p||p) == 0", kl_term(tgt_logits, t_ids, t_lp), torch.zeros(N), 1e-4)
check("KL(p||p) == 0 at T=4", kl_term(tgt_logits, t_ids, t_lp, 4.0), torch.zeros(N), 1e-4)

# 3. non-negative
v = kl_term(stu, t_ids, t_lp)
print(f"  {'PASS' if float(v.min()) >= -1e-4 else 'FAIL'}  KL >= 0: min {float(v.min()):.3e}")
ok &= float(v.min()) >= -1e-4

# 4. T != 1 matches an explicit restricted-support computation
T = 4.0
lq = F.log_softmax(t_lp / T, -1)
lp = F.log_softmax(stu.gather(-1, t_ids) / T, -1)
check("T=4 == explicit top-K KL x T^2", kl_term(stu, t_ids, t_lp, T),
      T * T * (lq.exp() * (lq - lp)).sum(-1), 1e-4)

# 5. padded teacher rows (-1e30) must not poison the result
t_lp2, t_ids2 = t_lp.clone(), t_ids.clone()
t_lp2[:, -8:] = -1e30
t_ids2[:, -8:] = 0
r = kl_term(stu, t_ids2, t_lp2)
print(f"  {'PASS' if torch.isfinite(r).all() else 'FAIL'}  padded rows stay finite")
ok &= bool(torch.isfinite(r).all())

# 6. the property the whole objective rests on: a student that puts its mass
#    OUTSIDE the teacher's support (a rank>1000 proposal) must be punished far
#    harder than one that merely picks the teacher's rank-2 token.
peaked = tgt_logits                       # near one-hot, as the real teacher is
copycat = peaked.clone()
near_miss = peaked.clone()                # swap top-1 and top-2: a near tie
near_miss[torch.arange(N), t_ids[:, 0]] = peaked[torch.arange(N), t_ids[:, 1]]
near_miss[torch.arange(N), t_ids[:, 1]] = peaked[torch.arange(N), t_ids[:, 0]]
far_miss = torch.full((N, V), -20.0)      # all mass on an off-support token
far_miss[torch.arange(N), (t_ids[:, 0] + 1000) % V] = 20.0
a = float(kl_term(copycat, t_ids, t_lp).mean())
b = float(kl_term(near_miss, t_ids, t_lp).mean())
c = float(kl_term(far_miss, t_ids, t_lp).mean())
print(f"  {'PASS' if c > b > a else 'FAIL'}  off-support mass is punished "
      f"hardest: copycat {a:.3f} < near-miss {b:.3f} < off-support {c:.3f}")
ok &= c > b > a

print("ALL PASS" if ok else "FAILURES")
sys.exit(0 if ok else 1)
