#!/usr/bin/env python3
"""Score every gate against ground truth.

Two numbers per gate, and they are not interchangeable:

  recall     of the samples where THIS defect was injected, how many did the
             gate catch
  FP rate    of the clean samples, how many did the gate fire on anyway

A gate that fires on everything has perfect recall and is worthless, which is
why both are reported and neither is averaged into a score.
"""
import collections
import json
import pathlib
import sys

import gates

HERE = pathlib.Path(__file__).resolve().parent
GEOM = HERE / "geom"


def fired(doc):
    """Which gates returned at least one FAIL."""
    out = collections.Counter()
    for f in gates.run(doc):
        if f.verdict == gates.FAIL:
            out[f.gate] += 1
    return out


def _key(doc):
    return sorted((w["i"], w["x"], w["y"], w["w"], w["h"], w.get("fg"), w.get("bg"))
                  for w in doc["widgets"])


def main():
    labels = json.loads((HERE / "samples" / "labels.json").read_text())
    good, bad, inert = {}, {}, []
    for sid, meta in labels.items():
        g = GEOM / f"{sid}.json"
        b = GEOM / f"{sid}-{meta['mutation']}.json"
        if not (g.exists() and b.exists()):
            continue
        gd, bd = json.loads(g.read_text()), json.loads(b.read_text())
        # A mutation that renders identically did not inject anything, and
        # counting it as a miss would blame the gate for the generator. The
        # test is "did the render change" — independent of any gate.
        if _key(gd) == _key(bd):
            inert.append(sid)
            continue
        good[sid] = fired(gd)
        bad[sid] = fired(bd)
    for sid in inert:
        labels.pop(sid, None)
    if inert:
        print(f"dropped {len(inert)} inert mutations (rendered identically): "
              f"{', '.join(inert)}\n")

    names = sorted({g.__name__.replace("gate_", "") for g in gates.GATES})
    per_defect = collections.defaultdict(list)
    for sid, meta in labels.items():
        if sid in bad:
            per_defect[meta["expect_gate"]].append(sid)

    print(f"{len(good)} clean renders, {len(bad)} defective renders "
          f"({len(per_defect)} defect classes)\n")

    hdr = f"{'gate':<12}{'recall on its own defect':<28}{'fires on clean':<18}{'fires on other defects'}"
    print(hdr)
    print("-" * len(hdr))
    for name in names:
        tgt = per_defect.get(name, [])
        hit = sum(1 for sid in tgt if bad[sid].get(name))
        fp = sum(1 for sid in good if good[sid].get(name))
        other = [sid for sid, m in labels.items()
                 if sid in bad and m["expect_gate"] != name and bad[sid].get(name)]
        rec = f"{hit}/{len(tgt)}" + (f"  ({100*hit/len(tgt):.0f}%)" if tgt else "   (not injected)")
        print(f"{name:<12}{rec:<28}{f'{fp}/{len(good)}  ({100*fp/max(1,len(good)):.0f}%)':<18}"
              f"{len(other)}/{len(bad)-len(tgt)}")

    # Overall, excluding gates that fire on every render — a constant is not a
    # discriminator, whatever it is measuring.
    constant = {n for n in names
                if sum(1 for s in good if good[s].get(n)) == len(good) and good}
    if constant:
        print(f"\nconstant on every clean render (no discriminating power here): "
              f"{', '.join(sorted(constant))}")

    disc = [n for n in names if n not in constant]
    caught = sum(1 for sid, m in labels.items()
                 if sid in bad and any(bad[sid].get(n) for n in disc))
    clean = sum(1 for sid in good if not any(good[sid].get(n) for n in disc))
    print(f"\nwith the constant gates set aside:")
    print(f"  defects caught by SOME gate   {caught}/{len(bad)}  ({100*caught/max(1,len(bad)):.0f}%)")
    print(f"  clean renders left alone      {clean}/{len(good)}  ({100*clean/max(1,len(good)):.0f}%)")

    if "-v" in sys.argv:
        print("\nmisses:")
        for sid, m in sorted(labels.items()):
            if sid in bad and not bad[sid].get(m["expect_gate"]):
                print(f"  {sid} {m['mutation']:<10} {m['card']:<26} fired: {dict(bad[sid])}")
        print("\nfalse positives on clean:")
        for sid in sorted(good):
            f = {k: v for k, v in good[sid].items() if k not in constant}
            if f:
                print(f"  {sid} {labels[sid]['card']:<26} {f}")


if __name__ == "__main__":
    main()
