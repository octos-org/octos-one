#!/usr/bin/env python3
"""Step 5: divergence, validity and speed across the arms.

Runs inside the serving image (needs the target tokenizer for token-level
divergence). Reads arm_*.json from draft-training/stylist/ and writes
analysis.json + a printed report.
"""
from __future__ import annotations
import argparse, difflib, json, os, re, statistics as st, collections, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from score_cards import structural                      # noqa: E402

BASE = "/home/ubuntu/qwen38-h200"
OUT = os.path.join(BASE, "draft-training", "stylist")

# ------------------------------------------------------------- line taxonomy
STRUCT_RE = re.compile(
    r"^\s*(view\b|component\b|when\b|for\b|[A-Z][A-Za-z0-9]*\s*[({]|\}|\{)")
VALUE_RE = re.compile(r"^\s*(source\b|state\b|event\b)")
STYLE_RE = re.compile(r"^\s*(copy\b|theme\b|style\b|palette\b)")
STYLE_ARG_RE = re.compile(r"\b(gap|pad|size|width|align|weight|color|font|radius|mood)\s*:")


def classify(line: str) -> str:
    if STYLE_RE.match(line):
        return "style"
    if VALUE_RE.match(line):
        return "value"
    if STYLE_ARG_RE.search(line) and STRUCT_RE.match(line):
        return "style"
    if STRUCT_RE.match(line):
        return "structure"
    if line.strip().startswith("#"):
        return "header"
    return "other"


# ------------------------------------------------------------------ validity
def known_widgets(harvest):
    seen = collections.Counter()
    for l in open(harvest):
        if not l.strip():
            continue
        r = json.loads(l)
        if r["mode"] not in ("pick", "compose"):
            continue
        for w in re.findall(r"\b([A-Z][A-Za-z0-9]+)\s*\(", r["content"]):
            seen[w] += 1
    # a name used by at least 3 harvested cards is part of the catalogue
    return {w for w, n in seen.items() if n >= 3}


def validity(text, vocab):
    body = text
    fenced = bool(re.search(r"```runl0\s", body))
    if "```" in body:
        m = re.search(r"```runl0\s*\n(.*?)(?:\n```|$)", body, re.S)
        body = m.group(1) if m else re.sub(r"```[a-z0-9]*\n?", "", body)
    bal = {}
    for o, c, nm in (("{", "}", "brace"), ("[", "]", "bracket"), ("(", ")", "paren")):
        # count outside of double-quoted strings
        depth, mind, s, i = 0, 0, body, 0
        inq = False
        while i < len(s):
            ch = s[i]
            if ch == '"' and (i == 0 or s[i - 1] != "\\"):
                inq = not inq
            elif not inq:
                if ch == o:
                    depth += 1
                elif ch == c:
                    depth -= 1
                    mind = min(mind, depth)
            i += 1
        bal[nm] = (depth == 0 and mind == 0)
    views = set(re.findall(r"^view\s+(\w+)\s", body, re.M))
    comps = set(re.findall(r"^component\s+(\w+)\s*\(", body, re.M))
    # bare identifier lines inside blocks are view references
    refs = set()
    for l in body.splitlines():
        m = re.match(r"^\s+(\w+)\s*$", l)
        if m:
            refs.add(m.group(1))
    dangling = sorted(refs - views - comps - {"current"})
    used = set(re.findall(r"\b([A-Z][A-Za-z0-9]+)\s*\(", body))
    unknown = sorted(used - vocab - comps)
    quotes = body.count('"') - body.count('\\"')
    return {
        "fenced": fenced,
        "has_level": "# level: L0" in body,
        "has_root": bool(re.search(r"^view\s+root\b", body, re.M)),
        "balanced": all(bal.values()),
        "bal_detail": bal,
        "quotes_even": quotes % 2 == 0,
        "n_dangling_refs": len(dangling),
        "dangling": dangling[:6],
        "n_unknown_widgets": len(unknown),
        "unknown": unknown[:6],
        "n_view": len(views),
        "n_source": len(re.findall(r"^source\s+\S", body, re.M)),
    }


def is_valid(v):
    return (v["fenced"] and v["has_level"] and v["has_root"] and v["balanced"]
            and v["quotes_even"] and v["n_dangling_refs"] == 0
            and v["n_unknown_widgets"] == 0)


# ------------------------------------------------------------------ analysis
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=OUT)
    ap.add_argument("--ref", default="A_exact_card")
    ap.add_argument("--tokenizer", default="/models/target")
    args = ap.parse_args()

    tok = None
    try:
        from transformers import AutoTokenizer
        tok = AutoTokenizer.from_pretrained(args.tokenizer, trust_remote_code=True)
    except Exception as e:
        print(f"[analyze] no tokenizer ({e}); token metrics will be character-based")

    arms = {}
    for f in sorted(os.listdir(args.dir)):
        if f.startswith("arm_") and f.endswith(".json"):
            d = json.load(open(os.path.join(args.dir, f)))
            arms[d["arm"]] = d
    print(f"[analyze] arms: {list(arms)}")
    ref = arms[args.ref]
    refrow = {r["id"]: r for r in ref["rows"]}
    vocab = known_widgets(os.path.join(BASE, "harvest", "out.jsonl"))
    print(f"[analyze] widget vocabulary: {len(vocab)} names")

    report = {"ref": args.ref, "arms": {}}
    for name, d in arms.items():
        rows = d["rows"]
        tk = [r["tok_s"] for r in rows if r["tokens"] > 1]
        ntok = [r["tokens"] for r in rows]
        ent = {"cfg": d["cfg"], "n": len(rows),
               "tok_s_mean": round(st.mean(tk), 1) if tk else 0,
               "tok_s_median": round(st.median(tk), 1) if tk else 0,
               "tokens_mean": round(st.mean(ntok), 1),
               "finish_len": sum(1 for r in rows if r["finish"] == "length")}
        # per-slice speed
        bysl = collections.defaultdict(list)
        for r in rows:
            if r["tokens"] > 1:
                bysl[r["slice"]].append(r["tok_s"])
        ent["tok_s_by_slice"] = {k: round(st.mean(v), 1) for k, v in bysl.items()}

        # structural metrics of the emitted cards, model-free
        sts = [structural(r["text"]) for r in rows]
        keys = [k for k in sts[0] if not k.startswith("has_")]
        ent["struct_mean"] = {k: round(st.mean([x[k] for x in sts]), 2) for k in keys}
        ent["struct_mean"].update(
            {k: round(st.mean([float(x[k]) for x in sts]), 3)
             for k in sts[0] if k.startswith("has_")})

        # validity
        vals = [validity(r["text"], vocab) for r in rows]
        ent["valid_n"] = sum(1 for v in vals if is_valid(v))
        ent["valid_frac"] = round(ent["valid_n"] / max(1, len(vals)), 3)
        fails = collections.Counter()
        for v in vals:
            for k in ("fenced", "has_level", "has_root", "balanced", "quotes_even"):
                if not v[k]:
                    fails[k] += 1
            if v["n_dangling_refs"]:
                fails["dangling_refs"] += 1
            if v["n_unknown_widgets"]:
                fails["unknown_widgets"] += 1
        ent["validity_failures"] = dict(fails)

        # divergence vs the reference arm
        if name != args.ref:
            same, difftok, tottok, firsts, buckets = 0, 0, 0, [], collections.Counter()
            per = []
            for r in rows:
                a = refrow.get(r["id"])
                if a is None:
                    continue
                if a["text"] == r["text"]:
                    same += 1
                if tok is not None:
                    ta = tok.encode(a["text"], add_special_tokens=False)
                    tb = tok.encode(r["text"], add_special_tokens=False)
                else:
                    ta, tb = list(a["text"]), list(r["text"])
                sm = difflib.SequenceMatcher(None, ta, tb, autojunk=False)
                matched = sum(bl.size for bl in sm.get_matching_blocks())
                n = max(len(ta), len(tb))
                difftok += n - matched
                tottok += n
                lcp = 0
                for x, y in zip(ta, tb):
                    if x != y:
                        break
                    lcp += 1
                firsts.append(lcp / max(1, len(ta)))
                per.append({"id": r["id"], "slice": r["slice"],
                            "identical": a["text"] == r["text"],
                            "tok_change": round((n - matched) / max(1, n), 4),
                            "first_div_frac": round(lcp / max(1, len(ta)), 4),
                            "len_ref": len(ta), "len_arm": len(tb)})
                # where do the changed regions land?
                la, lb = a["text"].splitlines(), r["text"].splitlines()
                lsm = difflib.SequenceMatcher(None, la, lb, autojunk=False)
                for tag, i1, i2, j1, j2 in lsm.get_opcodes():
                    if tag == "equal":
                        continue
                    for l in la[i1:i2]:
                        buckets[classify(l)] += 1
                    for l in lb[j1:j2]:
                        buckets[classify(l)] += 1
            ent["identical_n"] = same
            ent["identical_frac"] = round(same / max(1, len(rows)), 3)
            ent["outputs_changed_frac"] = round(1 - same / max(1, len(rows)), 3)
            ent["tokens_changed_frac"] = round(difftok / max(1, tottok), 4)
            ent["first_div_frac_median"] = round(st.median(firsts), 3) if firsts else None
            ent["changed_lines_by_kind"] = dict(buckets)
            ent["per_query"] = per
        report["arms"][name] = ent

    # structural deltas vs the reference arm
    refst = report["arms"][args.ref]["struct_mean"]
    for name, e in report["arms"].items():
        if name == args.ref:
            continue
        e["struct_delta"] = {k: round(e["struct_mean"][k] - refst[k], 3)
                             for k in refst}

    json.dump(report, open(os.path.join(args.dir, "analysis.json"), "w"), indent=1)
    print(f"\n{'arm':22s} {'tok/s':>8s} {'tok':>7s} {'valid':>7s} {'ident':>7s} "
          f"{'tokchg':>8s} {'firstdiv':>9s}")
    for name, e in report["arms"].items():
        print(f"{name:22s} {e['tok_s_mean']:8.1f} {e['tokens_mean']:7.0f} "
              f"{e['valid_frac']:7.2f} "
              f"{e.get('identical_frac', 1.0):7.2f} "
              f"{e.get('tokens_changed_frac', 0.0):8.4f} "
              f"{str(e.get('first_div_frac_median','-')):>9s}")
    print(f"\nwrote {os.path.join(args.dir,'analysis.json')}")


if __name__ == "__main__":
    main()
