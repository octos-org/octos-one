#!/usr/bin/env python3
"""Run the gates over the whole corpus, not a sample of it.

The gates were measured on 60 cards — one per model family — and found six real
defects in cards nobody had mutated. There are 967. This renders every one and
reports what the gates say, which is the difference between "the gates work" and
"here is the list".

Realization is offline and sources are baked to literals, so this needs no
network and no device: about a second per card.

Usage:  corpus_sweep.py [N] [--only <substring>]
"""
import collections
import json
import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import gates          # noqa: E402
import make_samples   # noqa: E402
import render         # noqa: E402
import synth_data     # noqa: E402

CORPUS = HERE.parent / "style-factory" / "corpus"
WORK = HERE / "sweep"
SPLASH = pathlib.Path.home() / "home" / "Splash"


def realize(card):
    data = synth_data.synth(card.read_text())
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(data, f)
        path = f.name
    try:
        r = subprocess.run(
            ["cargo", "run", "-q", "-p", "splash-ui-l0", "--example", "lower_l0",
             "--", str(card), path],
            cwd=SPLASH, capture_output=True, text=True, timeout=120)
    except subprocess.TimeoutExpired:
        return None
    finally:
        pathlib.Path(path).unlink(missing_ok=True)
    if r.returncode != 0 or not r.stdout.strip():
        return None
    dsl = make_samples.bake(r.stdout)
    return "\n".join(l for l in dsl.splitlines() if not l.startswith("//")).strip()


def main():
    n = None
    only = None
    args = sys.argv[1:]
    if args and args[0].isdigit():
        n = int(args[0])
    if "--only" in args:
        only = args[args.index("--only") + 1]

    (WORK / "dsl").mkdir(parents=True, exist_ok=True)
    (WORK / "geom").mkdir(parents=True, exist_ok=True)
    cards = sorted(CORPUS.glob("*.card"))
    if only:
        cards = [c for c in cards if only in c.name]
    if n:
        cards = cards[:n]

    findings = collections.defaultdict(list)
    unrealized, unrendered, clean = [], [], 0

    for i, card in enumerate(cards, 1):
        dsl_path = WORK / "dsl" / f"{card.stem}.dsl"
        geom = WORK / "geom" / f"{card.stem}.json"
        if not geom.exists():
            if not dsl_path.exists():
                dsl = realize(card)
                if dsl is None:
                    unrealized.append(card.name)
                    print(f"  [{i}/{len(cards)}] {card.stem:<28} no realize", flush=True)
                    continue
                dsl_path.write_text(dsl)
            if render.render(dsl_path, geom) != "ok":
                unrendered.append(card.name)
                print(f"  [{i}/{len(cards)}] {card.stem:<28} no render", flush=True)
                continue

        doc = json.loads(geom.read_text())
        hits = collections.Counter()
        for f in gates.run(doc):
            # tap_target is a property of the renderer, not of a card — it fires
            # on every render, so counting it here would drown the signal.
            if f.verdict == gates.FAIL and f.gate != "tap_target":
                hits[f.gate] += 1
                findings[f.gate].append((card.stem, f.detail))
        if hits:
            print(f"  [{i}/{len(cards)}] {card.stem:<28} "
                  + " ".join(f"{k}x{v}" for k, v in hits.items()), flush=True)
        else:
            clean += 1

    total = clean + len({c for g in findings.values() for c, _ in g})
    print(f"\n{'=' * 62}")
    print(f"{len(cards)} cards · {total} rendered · {clean} clean "
          f"· {len(unrealized)} did not realize · {len(unrendered)} did not render\n")
    for gate, items in sorted(findings.items(), key=lambda kv: -len(kv[1])):
        cards_hit = len({c for c, _ in items})
        print(f"  {gate:<11} {len(items):4} findings across {cards_hit} cards")
    (WORK / "findings.json").write_text(json.dumps(
        {k: v for k, v in findings.items()}, indent=1))
    print(f"\n-> {WORK / 'findings.json'}")


if __name__ == "__main__":
    main()
