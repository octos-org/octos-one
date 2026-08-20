#!/usr/bin/env python3
"""Print concrete A-vs-C snippets for STYLIST.md: the queries where the lenient
arm diverged most, with a few lines of unified diff around the first change."""
import argparse, difflib, json, os, sys

BASE = "/home/ubuntu/qwen38-h200"
OUT = os.path.join(BASE, "draft-training", "stylist")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ref", default="A_exact_card")
    ap.add_argument("--arm", required=True)
    ap.add_argument("--dir", default=OUT)
    ap.add_argument("--n", type=int, default=3)
    ap.add_argument("--context", type=int, default=2)
    ap.add_argument("--max-hunks", type=int, default=3)
    ap.add_argument("--pick", default="", help="comma list of ids to force")
    args = ap.parse_args()

    ref = {r["id"]: r for r in
           json.load(open(os.path.join(args.dir, f"arm_{args.ref}.json")))["rows"]}
    arm = json.load(open(os.path.join(args.dir, f"arm_{args.arm}.json")))["rows"]
    an = json.load(open(os.path.join(args.dir, "analysis.json")))
    per = {p["id"]: p for p in an["arms"][args.arm].get("per_query", [])}

    rows = [r for r in arm if r["id"] in ref and ref[r["id"]]["text"] != r["text"]]
    if args.pick:
        want = args.pick.split(",")
        rows = [r for r in arm if r["id"] in want]
    else:
        rows.sort(key=lambda r: -per.get(r["id"], {}).get("tok_change", 0))
        rows = rows[: args.n]

    for r in rows:
        a = ref[r["id"]]
        p = per.get(r["id"], {})
        print("=" * 78)
        print(f"{r['id']}  [{r['slice']}]  {r['query']}")
        print(f"  tokens changed {100*p.get('tok_change',0):.1f}%, first divergence at "
              f"{100*p.get('first_div_frac',0):.1f}% of the reference output; "
              f"len {p.get('len_ref')} -> {p.get('len_arm')} tokens")
        la, lb = a["text"].splitlines(), r["text"].splitlines()
        sm = difflib.SequenceMatcher(None, la, lb, autojunk=False)
        shown = 0
        for tag, i1, i2, j1, j2 in sm.get_opcodes():
            if tag == "equal" or shown >= args.max_hunks:
                continue
            shown += 1
            c = args.context
            print(f"  --- hunk {shown} (A lines {i1+1}-{i2}, C lines {j1+1}-{j2}) ---")
            for l in la[max(0, i1 - c): i1]:
                print("     " + l)
            for l in la[i1:i2][:12]:
                print("  A- " + l)
            for l in lb[j1:j2][:12]:
                print("  C+ " + l)
            for l in la[i2: i2 + c]:
                print("     " + l)
        print()


if __name__ == "__main__":
    main()
