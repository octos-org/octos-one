#!/usr/bin/env python3
"""Extract unique card sources from the harvest for the beauty pipeline.

General-mode records are prose, not cards — skipped. Duplicate outputs (the
DASHBOARDS templates that ignored {c} produced byte-identical cards) are
deduped by content hash. Compose cards are listed first in meta.jsonl so the
render loop covers the family with the most design variance before the picks.
"""
import hashlib
import json
import re
from pathlib import Path

BASE = Path(__file__).resolve().parent
HARVEST = BASE.parent / "harvest_out.jsonl"
FENCE = re.compile(r"```runl0\s*\n(.*?)```", re.S)

records = [json.loads(line) for line in open(HARVEST)]
seen: dict[str, str] = {}
meta = []
no_fence = 0

for r in records:
    if r["mode"] == "general":
        continue
    m = FENCE.search(r["content"])
    if not m:
        no_fence += 1
        continue
    src = m.group(1).strip() + "\n"
    digest = hashlib.md5(src.encode()).hexdigest()[:12]
    if digest in seen:
        continue
    seen[digest] = r["id"]
    (BASE / "cards" / f"{r['id']}.card").write_text(src)
    meta.append({
        "id": r["id"],
        "family": r["family"],
        "mode": r["mode"],
        "query": r["query"],
        "hash": digest,
        "tokens": r["completion_tokens"],
    })

meta.sort(key=lambda m: (m["mode"] != "compose", m["id"]))
with open(BASE / "meta.jsonl", "w") as f:
    for m in meta:
        f.write(json.dumps(m) + "\n")

by_mode = {}
for m in meta:
    by_mode[m["mode"]] = by_mode.get(m["mode"], 0) + 1
print(f"unique cards: {len(meta)}  by mode: {by_mode}  no-fence skipped: {no_fence}")
