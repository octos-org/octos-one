#!/usr/bin/env python3
"""Build the evaluation slices the Goal-5 gates are stated against.

  cards           held-out pick+compose sequences (seen query shapes)
  unseen-combos   compose sequences whose (template, city) PAIR is never trained
                  on, while that template and that city each appear in training
                  with other partners. This is the real test of the thesis: can
                  the draft compose sections it has never seen composed together.
  general         held-out prose/summary sequences

The job matrix is regenerated from harvest.py's own lists so the (template, city)
factorisation is exact rather than guessed from the query string.
"""
from __future__ import annotations
import argparse, json, os, random, re, collections

BASE = "/home/ubuntu/qwen38-h200"

CITIES = ("Tokyo London Paris Berlin Madrid Rome Vienna Prague Lisbon Athens Cairo Oslo "
    "Helsinki Warsaw Dublin Zurich Porto Naples Lyon Turin Seville Genoa Nice Basel Ghent "
    "Leeds Bergen Aarhus Graz Brno Kyoto Osaka Seoul Busan Taipei Singapore Bangkok Hanoi "
    "Mumbai Delhi Nairobi Lagos Casablanca Istanbul Dubai Doha Sydney Melbourne Auckland "
    "Toronto Vancouver Montreal Chicago Boston Seattle Denver Austin Miami Havana Lima "
    "Bogota Santiago Quito Reykjavik Tallinn Riga Vilnius Krakow Zagreb Belgrade Sofia").split()

TRAVEL_COMBOS = [
    "a travel page for {c}: current weather, top things to do, and how to get around",
    "plan a trip to {c} — weather this week, local events, and videos about the city",
    "a {c} city guide card: weather, activities, and directions from the airport",
    "one page for my {c} visit: forecast, what to do today, and a news section about {c}",
    "compose a {c} dashboard: weather, nearby activities, and travel videos",
]
DASHBOARDS = [
    "my morning dashboard: weather here, top stocks, and top headlines",
    "a market morning card: NVDA and AAPL tiles plus business news",
    "commute card: weather now plus directions to the office",
    "an evening card: weather tonight and jazz videos",
    "a weekend planner: weather saturday, things to do, and event videos",
    "tech pulse: chip stocks and semiconductor news in one card",
    "a storm tracker: weather, earthquake feed, and emergency news",
    "a foodie card for {c}: nearby restaurants and food videos",
    "a runner's card: weather, air quality, and a route to the park",
    "compare {a} and {b} gdp plus their market news",
]


def compose_factors():
    """query -> (template_key, city) for every compose job in the matrix."""
    out = {}
    for c in CITIES[:56]:
        for ti, t in enumerate(TRAVEL_COMBOS):
            out[t.format(c=c)] = (f"travel{ti}", c)
    for di, d in enumerate(DASHBOARDS):
        for c in CITIES[:30]:
            q = d.format(c=c, a="china", b="india")
            # templates without {c} collapse to one query repeated per city; the
            # first city to claim it owns it, the rest are exact duplicates
            out.setdefault(q, (f"dash{di}", c))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--harvest", default=os.path.join(BASE, "harvest", "out.jsonl"))
    ap.add_argument("--out", default="/home/ubuntu/qwen38-h200/draft-training/splits")
    ap.add_argument("--card-frac", type=float, default=0.07)
    ap.add_argument("--general-frac", type=float, default=0.10)
    ap.add_argument("--unseen-pairs", type=int, default=60,
                    help="how many (template, city) compose pairs to withhold")
    ap.add_argument("--seed", type=int, default=17)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    rng = random.Random(args.seed)

    recs = [json.loads(l) for l in open(args.harvest) if l.strip()]
    recs = [r for r in recs if (r.get("content") or "").strip()]
    fac = compose_factors()

    comp = [r for r in recs if r["mode"] == "compose"]
    pick = [r for r in recs if r["mode"] == "pick"]
    gen = [r for r in recs if r["mode"] == "general"]

    # --- unseen combos: hold out (template, city) pairs, keeping both factors
    #     represented in training via their other partners
    pairs = {}
    for r in comp:
        f = fac.get(r["query"])
        if f:
            pairs.setdefault(f, []).append(r["id"])
    tmpl_count = collections.Counter(t for (t, c) in pairs)
    city_count = collections.Counter(c for (t, c) in pairs)
    cand = [p for p in pairs if tmpl_count[p[0]] >= 3 and city_count[p[1]] >= 2]
    rng.shuffle(cand)
    chosen, used_t, used_c = [], collections.Counter(), collections.Counter()
    for (t, c) in cand:
        if len(chosen) >= args.unseen_pairs:
            break
        # never withhold so many that a template or city vanishes from training
        if used_t[t] + 1 >= tmpl_count[t] or used_c[c] + 1 >= city_count[c]:
            continue
        chosen.append((t, c)); used_t[t] += 1; used_c[c] += 1
    unseen_ids = sorted(i for p in chosen for i in pairs[p])

    rest_cards = [r["id"] for r in comp + pick if r["id"] not in set(unseen_ids)]
    rng.shuffle(rest_cards)
    n_cards = max(1, int(len(rest_cards) * args.card_frac))
    cards_ho = sorted(rest_cards[:n_cards])

    gids = [r["id"] for r in gen]
    rng.shuffle(gids)
    gen_ho = sorted(gids[: max(1, int(len(gids) * args.general_frac))])

    splits = {"cards": cards_ho, "unseen_combos": unseen_ids, "general": gen_ho}
    holdout = sorted(set(cards_ho) | set(unseen_ids) | set(gen_ho))
    train = sorted({r["id"] for r in recs} - set(holdout))

    for k, v in splits.items():
        json.dump(v, open(os.path.join(args.out, f"holdout_{k}.json"), "w"))
    json.dump(holdout, open(os.path.join(args.out, "holdout_all.json"), "w"))
    json.dump(train, open(os.path.join(args.out, "train.json"), "w"))
    slice_map = {}
    for k, v in splits.items():
        for i in v:
            slice_map[i] = k
    json.dump(slice_map, open(os.path.join(args.out, "slices.json"), "w"))

    print(f"records: pick={len(pick)} compose={len(comp)} general={len(gen)}")
    print(f"compose (template, city) pairs: {len(pairs)}; withheld {len(chosen)} "
          f"-> {len(unseen_ids)} unseen-combo sequences")
    print(f"cards holdout {len(cards_ho)}, general holdout {len(gen_ho)}, "
          f"total holdout {len(holdout)}, train {len(train)}")
    tset = {t for (t, c) in chosen}
    cset = {c for (t, c) in chosen}
    tr_pairs = {p for p in pairs if p not in set(chosen)}
    bad_t = [t for t in tset if not any(p[0] == t for p in tr_pairs)]
    bad_c = [c for c in cset if not any(p[1] == c for p in tr_pairs)]
    print(f"withheld templates still seen in training: {len(tset)-len(bad_t)}/{len(tset)}"
          + (f"  MISSING {bad_t}" if bad_t else ""))
    print(f"withheld cities still seen in training: {len(cset)-len(bad_c)}/{len(cset)}"
          + (f"  MISSING {bad_c}" if bad_c else ""))
    print(f"wrote splits to {args.out}")


if __name__ == "__main__":
    main()
