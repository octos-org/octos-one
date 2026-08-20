#!/usr/bin/env python3
"""Step 2 prep: turn judge scores into the stylist training set.

Selection rules:
  * candidates = cards that are in the ORIGINAL train split (never train on a
    held-out query) and whose target features actually extracted;
  * "best" = top quartile by judge SCORE, ties broken by the sum of the five
    sub-scores (the judge's SCORE lands on a coarse lattice, so the sub-score
    sum is a genuine tiebreak, not noise);
  * the general slice is kept whole -- it is the forgetting protection that made
    general acc@48 go UP in the card run.

Also reports how much the ranking is really just "longer card", which is the
main thing that could make this experiment vacuous.
"""
from __future__ import annotations
import argparse, json, math, os, statistics as st, collections

BASE = "/home/ubuntu/qwen38-h200"
HERE = os.path.dirname(os.path.abspath(__file__))


def spearman(a, b):
    def rank(x):
        order = sorted(range(len(x)), key=lambda i: x[i])
        r = [0.0] * len(x)
        i = 0
        while i < len(order):
            j = i
            while j + 1 < len(order) and x[order[j + 1]] == x[order[i]]:
                j += 1
            avg = (i + j) / 2.0 + 1
            for t in range(i, j + 1):
                r[order[t]] = avg
            i = j + 1
        return r
    ra, rb = rank(a), rank(b)
    ma, mb = st.mean(ra), st.mean(rb)
    num = sum((x - ma) * (y - mb) for x, y in zip(ra, rb))
    den = math.sqrt(sum((x - ma) ** 2 for x in ra) * sum((y - mb) ** 2 for y in rb))
    return num / den if den else 0.0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scores", default=os.path.join(HERE, "scores.jsonl"))
    ap.add_argument("--splits-in", default=os.path.join(BASE, "draft-training", "splits"))
    ap.add_argument("--out", default=os.path.join(BASE, "draft-training", "splits_stylist"))
    ap.add_argument("--quantile", type=float, default=0.25)
    ap.add_argument("--mode", default="quartile", choices=("quartile", "weighted"))
    ap.add_argument("--stratify", type=int, default=1,
                    help="take the top quantile WITHIN each of pick/compose. The "
                         "judge rates single-app weather cards far above composed "
                         "ones, so a global cut selects 94%% pick and would make the "
                         "stylist draft differ from the card draft in card FAMILY "
                         "rather than in taste -- an unusable confound for an "
                         "experiment whose eval set is compose queries.")
    ap.add_argument("--weight-reps", type=int, default=4,
                    help="weighted mode: max repeats for the best-scoring card")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    rows = [json.loads(l) for l in open(args.scores) if l.strip()]
    rows = [r for r in rows if r["judge"]]
    print(f"[splits] {len(rows)} scored cards")

    train = set(json.load(open(os.path.join(args.splits_in, "train.json"))))
    hold = json.load(open(os.path.join(args.splits_in, "holdout_all.json")))

    sc = [r["judge"]["score"] for r in rows]
    print(f"[splits] SCORE: min {min(sc)} p25 {st.quantiles(sc, n=4)[0]:.0f} "
          f"med {st.median(sc):.0f} p75 {st.quantiles(sc, n=4)[2]:.0f} max {max(sc)} "
          f"mean {st.mean(sc):.1f} sd {st.pstdev(sc):.1f} distinct {len(set(sc))}")
    hist = collections.Counter(sc)
    print("[splits] score histogram: " +
          " ".join(f"{k}:{v}" for k, v in sorted(hist.items())))
    for m in ("pick", "compose"):
        xs = [r["judge"]["score"] for r in rows if r["mode"] == m]
        print(f"  {m:8s} n={len(xs):4d} mean {st.mean(xs):.1f} sd {st.pstdev(xs):.1f}")

    # how much of the ranking is length / structure rather than taste?
    print("[splits] spearman(SCORE, x):")
    for key, get in (("completion_tokens", lambda r: r["completion_tokens"]),
                     ("n_lines", lambda r: r["struct"]["n_lines"]),
                     ("n_source", lambda r: r["struct"]["n_source"]),
                     ("n_view", lambda r: r["struct"]["n_view"]),
                     ("n_root_sections", lambda r: r["struct"]["n_root_sections"]),
                     ("n_widget_kinds", lambda r: r["struct"]["n_widget_kinds"]),
                     ("n_image", lambda r: r["struct"]["n_image"]),
                     ("n_i18n", lambda r: r["struct"]["n_i18n"]),
                     ("has_theme", lambda r: int(r["struct"]["has_theme"]))):
        print(f"    {key:20s} {spearman(sc, [get(r) for r in rows]):+.3f}")

    cand = [r for r in rows if r["has_feats"] and r["id"] in train]
    print(f"[splits] {len(cand)} cards are trainable (have features, in train split)")
    key = lambda r: (r["judge"]["score"],
                     sum(r["judge"][k] for k in
                         ("hierarchy", "richness", "theme", "imagery", "density")))
    cand.sort(key=key, reverse=True)

    gen_ids = [json.loads(l)["id"] for l in open(os.path.join(BASE, "harvest", "out.jsonl"))
               if l.strip() and json.loads(l)["mode"] == "general"]
    gen_train = [g for g in gen_ids if g in train]

    if args.mode == "quartile":
        if args.stratify:
            chosen = []
            for m in ("pick", "compose"):
                sub = [r for r in cand if r["mode"] == m]
                nm = max(1, int(len(sub) * args.quantile))
                chosen += sub[:nm]
                print(f"[splits] {m}: {len(sub)} trainable -> top {nm}, "
                      f"cut SCORE {sub[nm-1]['judge']['score']}, "
                      f"mean {st.mean([r['judge']['score'] for r in sub[:nm]]):.1f} "
                      f"vs {st.mean([r['judge']['score'] for r in sub]):.1f} overall")
            chosen.sort(key=key, reverse=True)
        else:
            n = max(1, int(len(cand) * args.quantile))
            chosen = cand[:n]
        sel = [r["id"] for r in chosen]
        cut = key(chosen[-1])
        print(f"[splits] top quantile = {len(chosen)} cards, lowest kept SCORE "
              f"{cut[0]} (sub-sum {cut[1]}); selected score mean "
              f"{st.mean([r['judge']['score'] for r in chosen]):.1f} vs "
              f"{st.mean([r['judge']['score'] for r in cand]):.1f} for all cards")
        print("[splits] selected mix: " +
              str(collections.Counter(r["mode"] for r in chosen)))
        print("[splits] selected families: " +
              str(collections.Counter(r["family"] for r in chosen).most_common(8)))
    else:
        lo, hi = min(sc), max(sc)
        sel = []
        for r in cand:
            w = (r["judge"]["score"] - lo) / max(1e-9, hi - lo)
            reps = 1 + int(round(w * (args.weight_reps - 1)))
            sel += [r["id"]] * reps
        print(f"[splits] weighted: {len(cand)} cards -> {len(sel)} draws")

    train_list = sel + gen_train
    json.dump(train_list, open(os.path.join(args.out, "train.json"), "w"))
    json.dump(hold, open(os.path.join(args.out, "holdout_all.json"), "w"))
    for f in ("holdout_cards.json", "holdout_unseen_combos.json",
              "holdout_general.json", "slices.json"):
        src = os.path.join(args.splits_in, f)
        if os.path.exists(src):
            json.dump(json.load(open(src)), open(os.path.join(args.out, f), "w"))
    meta = {"mode": args.mode, "quantile": args.quantile,
            "n_cards_selected": len(set(sel)), "n_card_draws": len(sel),
            "n_general": len(gen_train), "n_train_draws": len(train_list),
            "selected_ids": sorted(set(sel))}
    json.dump(meta, open(os.path.join(args.out, "meta.json"), "w"), indent=1)
    print(f"[splits] wrote {args.out}: {len(sel)} card draws + {len(gen_train)} "
          f"general = {len(train_list)} train draws, {len(hold)} held out")


if __name__ == "__main__":
    main()
