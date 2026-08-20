#!/usr/bin/env python3
"""Goal 1: harvest/STATS.md — counts per family/mode, token totals, length
histograms, empty/error outputs, and 3 eyeballed compose-mode examples.

Pure stdlib: runs on the host python (no torch/transformers needed).
"""
import json, os, sys, collections, re

BASE = "/home/ubuntu/qwen38-h200"
OUT = os.path.join(BASE, "harvest", "out.jsonl")
STATS = os.path.join(BASE, "harvest", "STATS.md")

recs, bad_lines = [], 0
for line in open(OUT):
    line = line.strip()
    if not line:
        continue
    try:
        recs.append(json.loads(line))
    except Exception:
        bad_lines += 1

n = len(recs)
ids = [r["id"] for r in recs]
dup = n - len(set(ids))

by_mode = collections.Counter(r["mode"] for r in recs)
by_family = collections.Counter(r["family"] for r in recs)
by_mode_family = collections.Counter((r["mode"], r["family"]) for r in recs)

empty = [r for r in recs if not (r.get("content") or "").strip()]
# truncation: server max_tokens is 4096 for card modes, 1500 for general
def cap(r):
    return 1500 if r["mode"] == "general" else 4096
truncated = [r for r in recs if (r.get("completion_tokens") or 0) >= cap(r)]

ctoks = [r.get("completion_tokens") or 0 for r in recs]
ptoks = [r.get("prompt_tokens") or 0 for r in recs]
dts = [r.get("dt") or 0.0 for r in recs]

def pct(xs, p):
    if not xs: return 0
    s = sorted(xs)
    return s[min(len(s) - 1, int(round(p / 100.0 * (len(s) - 1))))]

def stat_block(xs):
    if not xs: return "n/a"
    return (f"n={len(xs)} min={min(xs)} p50={pct(xs,50)} p90={pct(xs,90)} "
            f"p99={pct(xs,99)} max={max(xs)} mean={sum(xs)/len(xs):.1f}")

# histogram of completion token lengths
def hist(xs, edges):
    c = collections.Counter()
    for x in xs:
        for e in edges:
            if x < e:
                c[e] += 1
                break
        else:
            c["inf"] += 1
    return c

EDGES = [128, 256, 512, 768, 1024, 1536, 2048, 3072, 4096]

# --- card structure probes -------------------------------------------------
FENCE = re.compile(r"^```runl0", re.M)
SOURCE = re.compile(r"^source\s+(\S+)", re.M)
# a "section" in a composed card: top-level UI containers
SECTION = re.compile(r"^\s{0,4}(Panel|Card|Section|Col|Row|Header|Group)\b", re.M)
MODEL_LINE = re.compile(r"^#\s*model:\s*(.+)$", re.M)

def probe(r):
    c = r.get("content") or ""
    return {
        "fenced": bool(FENCE.search(c)),
        "n_sources": len(SOURCE.findall(c)),
        "n_sections": len(SECTION.findall(c)),
        "models": MODEL_LINE.findall(c),
        "chars": len(c),
    }

probes = {r["id"]: probe(r) for r in recs}

def mode_recs(m):
    return [r for r in recs if r["mode"] == m]

lines = []
A = lines.append
A("# harvest/STATS.md")
A("")
A(f"Generated from `{OUT}` — {n} records"
  + (f" ({bad_lines} unparseable lines)" if bad_lines else "")
  + (f" ({dup} duplicate ids)" if dup else ""))
A("")
A("Target job matrix was 1496 (pick=516, compose=580, general=400).")
A("")

A("## 1. Counts")
A("")
A("| mode | records | completion tokens | mean len | empty | truncated at cap |")
A("|---|---|---|---|---|---|")
for m in ("pick", "compose", "general"):
    rs = mode_recs(m)
    ct = [r.get("completion_tokens") or 0 for r in rs]
    A(f"| {m} | {len(rs)} | {sum(ct):,} | {(sum(ct)/len(ct) if ct else 0):.0f} | "
      f"{sum(1 for r in rs if not (r.get('content') or '').strip())} | "
      f"{sum(1 for r in rs if (r.get('completion_tokens') or 0) >= cap(r))} |")
A(f"| **all** | **{n}** | **{sum(ctoks):,}** | **{(sum(ctoks)/n if n else 0):.0f}** | "
  f"**{len(empty)}** | **{len(truncated)}** |")
A("")

A("### Per family (mode / family)")
A("")
A("| mode | family | n | tokens | mean | p50 | p90 | max |")
A("|---|---|---|---|---|---|---|---|")
for (m, f), c in sorted(by_mode_family.items()):
    rs = [r for r in recs if r["mode"] == m and r["family"] == f]
    ct = [r.get("completion_tokens") or 0 for r in rs]
    A(f"| {m} | {f} | {c} | {sum(ct):,} | {sum(ct)/len(ct):.0f} | {pct(ct,50)} | {pct(ct,90)} | {max(ct)} |")
A("")

A("## 2. Token totals and length distribution")
A("")
A(f"- completion tokens: {stat_block(ctoks)}")
A(f"- prompt tokens:     {stat_block(ptoks)}")
A(f"- **total completion tokens harvested: {sum(ctoks):,}**")
A(f"- wall time per generation (s): {stat_block([round(d,1) for d in dts])}")
A("")
A("### Completion-length histogram (all modes)")
A("")
A("| bucket (tokens) | count | bar |")
A("|---|---|---|")
h = hist(ctoks, EDGES)
prev = 0
for e in EDGES + ["inf"]:
    c = h.get(e, 0)
    label = f"{prev}-{e-1}" if e != "inf" else f"{EDGES[-1]}+"
    A(f"| {label} | {c} | {'#' * int(60.0 * c / max(1, max(h.values())))} |")
    if e != "inf":
        prev = e
A("")
A("### Completion-length histogram by mode")
A("")
A("| bucket | " + " | ".join(("pick", "compose", "general")) + " |")
A("|---|---|---|---|")
hs = {m: hist([r.get("completion_tokens") or 0 for r in mode_recs(m)], EDGES) for m in ("pick", "compose", "general")}
prev = 0
for e in EDGES + ["inf"]:
    label = f"{prev}-{e-1}" if e != "inf" else f"{EDGES[-1]}+"
    A(f"| {label} | " + " | ".join(str(hs[m].get(e, 0)) for m in ("pick", "compose", "general")) + " |")
    if e != "inf":
        prev = e
A("")

A("## 3. Errors / degenerate outputs")
A("")
A(f"- unparseable jsonl lines: {bad_lines}")
A(f"- duplicate ids: {dup}")
A(f"- empty content: {len(empty)}")
if empty:
    for r in empty[:10]:
        A(f"  - `{r['id']}` ({r['mode']}/{r['family']}) query={r['query']!r}")
A(f"- hit the max_tokens cap (likely truncated): {len(truncated)}")
if truncated:
    tm = collections.Counter((r["mode"], r["family"]) for r in truncated)
    for (m, f), c in sorted(tm.items(), key=lambda x: -x[1])[:12]:
        A(f"  - {m}/{f}: {c}")
A("")
nofence = [r for r in recs if r["mode"] != "general" and not probes[r["id"]]["fenced"]]
A(f"- card-mode records missing the ```` ```runl0 ```` fence: {len(nofence)}")
for r in nofence[:10]:
    A(f"  - `{r['id']}` ({r['mode']}/{r['family']}) query={r['query']!r} -> starts {((r.get('content') or '')[:80])!r}")
A("")

A("## 4. Composition structure (the point of the harvest)")
A("")
A("`source` declarations and top-level UI containers per card, pick vs compose.")
A("A composed card should show strictly more `source` lines and more sections")
A("than a picked one — that is the multi-domain signal the NGRAM trie cannot replay.")
A("")
A("| mode | mean #source | p50 #source | max #source | mean #sections | mean chars |")
A("|---|---|---|---|---|---|")
for m in ("pick", "compose"):
    rs = mode_recs(m)
    if not rs: continue
    ns = [probes[r["id"]]["n_sources"] for r in rs]
    nse = [probes[r["id"]]["n_sections"] for r in rs]
    ch = [probes[r["id"]]["chars"] for r in rs]
    A(f"| {m} | {sum(ns)/len(ns):.2f} | {pct(ns,50)} | {max(ns)} | {sum(nse)/len(nse):.2f} | {sum(ch)/len(ch):.0f} |")
A("")
mc = collections.Counter()
for r in mode_recs("compose"):
    for md in probes[r["id"]]["models"]:
        mc[md.strip()] += 1
if mc:
    A("Declared `# model:` values seen in compose mode (top 20):")
    A("")
    for k, v in mc.most_common(20):
        A(f"- `{k}` x{v}")
    A("")

A("## 5. Three compose-mode cards, eyeballed for multi-section structure")
A("")
comp = sorted(mode_recs("compose"), key=lambda r: -(probes[r["id"]]["n_sources"]))
picks = []
seen_fam = set()
for r in comp:
    if r["family"] in seen_fam and len(picks) < 3:
        continue
    picks.append(r); seen_fam.add(r["family"])
    if len(picks) == 3:
        break
for i, r in enumerate(picks, 1):
    p = probes[r["id"]]
    A(f"### {i}. `{r['id']}` — {r['family']} — {r['query']!r}")
    A("")
    A(f"- completion_tokens: {r.get('completion_tokens')}, chars: {p['chars']}, "
      f"#source: {p['n_sources']}, #sections: {p['n_sections']}, "
      f"declared models: {p['models']}")
    A("")
    srcs = SOURCE.findall(r.get("content") or "")
    A(f"- source names in order: {srcs}")
    A("")
    A("<details><summary>full card</summary>")
    A("")
    A("````")
    A((r.get("content") or "").rstrip())
    A("````")
    A("")
    A("</details>")
    A("")

A("## 6. What this means for training")
A("")
A(f"- Usable card-mode sequences: {len(mode_recs('pick')) + len(mode_recs('compose'))}, "
  f"general: {len(mode_recs('general'))}.")
A(f"- Prompt is a fixed ~{pct(ptoks,50):,}-token context in card modes; the user query is the "
  "last few tokens of it (see CONDITIONING.md §4), so a bounded draft window captures the "
  "copyable values.")
over = sum(1 for r in recs if r["mode"] != "general" and (r.get("completion_tokens") or 0) > 2048)
A(f"- Card generations longer than the draft's 2048-token sliding window: {over} "
  f"({100.0*over/max(1,len(recs)-len(mode_recs('general'))):.1f}% of card-mode). Past that point the "
  "user query is out of reach for draft layers 0-3.")
A("")

os.makedirs(os.path.dirname(STATS), exist_ok=True)
open(STATS, "w").write("\n".join(lines) + "\n")
print(f"wrote {STATS} ({len(lines)} lines) from {n} records")
