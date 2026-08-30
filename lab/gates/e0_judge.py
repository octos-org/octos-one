#!/usr/bin/env python3
"""E0: do mockups have a beauty gradient a judge can detect?

Everything downstream — extracting a palette, a type scale, a composition from a
mockup and rebuilding a card from it — assumes mockups differ in quality in a way
that can be read off. That assumption has never been tested. Mockup image quality
scored a uniform 8-9 in every earlier measurement, and a score that never varies
carries no information.

So: every pair, judged blind, in BOTH orders. Two numbers come out.

  self-agreement  the judge picks the same image when the order is swapped.
                  At 50% it is guessing and there is no gradient to extract.
                  Measured at 87% on rendered cards earlier, so the judge is
                  capable — this asks whether the MOCKUPS are separable.

  rank coherence  how well the pairwise wins collapse to one ordering. A
                  consistent ranking means the differences are real and shared;
                  a circular one means each comparison found something local.

Usage:  e0_judge.py [--workers N]
"""
import concurrent.futures as cf
import itertools
import json
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
POOL = HERE / "e0"
OUT = HERE / "e0_results.json"

PROMPT = """You are judging two UI design mockups for a phone card.

Read both images:
  A: {a}
  B: {b}

Which is the better DESIGN? Judge composition, typographic hierarchy, restraint,
and how confidently the eye is led — not which subject you find more interesting,
and not the content. Ignore that they show different data.

Answer on ONE line, exactly this shape and nothing else:
WINNER=<A or B> | <at most 12 words saying why>"""


def judge(a, b):
    r = subprocess.run(
        ["claude", "-p", PROMPT.format(a=a, b=b),
         "--model", "opus", "--allowedTools", "Read"],
        capture_output=True, text=True, timeout=600)
    m = re.search(r"WINNER\s*=\s*([AB])\s*\|\s*(.*)", r.stdout)
    if not m:
        return None, (r.stdout or r.stderr)[:120]
    return m.group(1), m.group(2).strip()[:70]


def main():
    workers = 6
    if "--workers" in sys.argv:
        workers = int(sys.argv[sys.argv.index("--workers") + 1])
    shots = sorted(p for p in POOL.glob("*.png") if "sheet" not in p.name)
    names = [p.stem.replace("-mockup", "") for p in shots]
    print(f"{len(shots)} mockups, {len(list(itertools.combinations(shots, 2)))} pairs, both orders\n")

    jobs = []
    for i, j in itertools.combinations(range(len(shots)), 2):
        jobs.append((i, j, 0))     # i as A
        jobs.append((i, j, 1))     # i as B — the same question, order swapped
    results = []

    def run(job):
        i, j, swap = job
        a, b = (shots[j], shots[i]) if swap else (shots[i], shots[j])
        w, why = judge(a, b)
        if w is None:
            return None
        # resolve the winner back to an index, whatever the presentation order
        first, second = (j, i) if swap else (i, j)
        return {"i": i, "j": j, "swap": swap,
                "winner": first if w == "A" else second, "why": why}

    with cf.ThreadPoolExecutor(workers) as ex:
        for n, r in enumerate(ex.map(run, jobs), 1):
            if r:
                results.append(r)
                print(f"  [{n}/{len(jobs)}] {names[r['i']][:22]:<24} vs {names[r['j']][:22]:<24} "
                      f"-> {names[r['winner']][:22]:<24} {r['why'][:40]}", flush=True)
            else:
                print(f"  [{n}/{len(jobs)}] unparsed", flush=True)

    OUT.write_text(json.dumps({"names": names, "results": results}, indent=1))
    print(f"\n{len(results)}/{len(jobs)} judged -> {OUT}")


if __name__ == "__main__":
    main()
