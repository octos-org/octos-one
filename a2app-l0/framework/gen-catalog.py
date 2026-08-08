"""Generate the agent-facing catalog from the normative TOML.

Generated, not written: the TOML is what `check_ui_l0` enforces, and a
hand-maintained copy is a second source of truth that drifts the first time
someone adds a role. `catalog.md` carries a marker saying so.
"""
import re, sys
from pathlib import Path

src = Path(sys.argv[1]).read_text()
out = ["# Catalog: every role and capability a card may name",
       "",
       "**Generated from `Splash/docs/ui-l0-constructors.toml` — do not edit.**",
       "That file is what the checker enforces; a hand-kept copy would drift the",
       "first time a role is added, and the card would be refused for using",
       "something this document promised.",
       ""]

roles, sources, kinds = [], [], []
# A section's body runs until the next `[section]` or a blank line, and COMMENT
# lines inside it are skipped rather than ending it.
#
# The pattern used to be `(?:[a-z_]+ *=.*\n)*` — a run of attribute lines with
# nothing else allowed — so the first `#` comment between two attributes ended the
# body and every attribute after it vanished. Silently: `Map` lost `at` and `zoom`
# the moment `at` was documented, and the generated catalog promised the model LESS
# than the checker accepts. A card is refused for what the documentation omitted,
# which is the hardest kind of refusal to diagnose from the card.
for m in re.finditer(r'^\[([A-Za-z.\"]+[^\]]*)\]\n((?:(?:[a-z_]+ *=.*|#.*)\n)*)', src, re.M):
    name, body = m.group(1), m.group(2)
    args = []
    for line in body.strip().splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        k, v = line.split("=", 1)
        # Against `line`, not `v`. A `[kinds.*]` section states its set as a
        # top-level `tokens = [...]`, and the split above had already eaten the
        # word `tokens` into `k` — so the search never matched, every shared
        # token set rendered as `?`, and the model was told `unit` exists
        # without being told one legal value of it.
        toks = re.search(r'tokens *= *\[([^\]]*)\]', line)
        kind = re.search(r'kind *= *"([a-z]+)"', v)
        desc = kind.group(1) if kind else "?"
        if toks:
            desc = " | ".join("." + t.strip().strip('"') for t in toks.group(1).split(","))
        args.append(f"{k.strip()}: {desc}")
    if name.startswith("sources."):
        # A source declares `args = ["lat", "lon", …]`, a list — not one entry
        # per argument the way a role does.
        def lst(key):
            m2 = re.search(key + r' *= *\[([^\]]*)\]', body)
            return [a.strip().strip('"') for a in m2.group(1).split(",") if a.strip()] if m2 else []
        # The answerable fields belong in the agent's copy too. Without them a
        # card can name a field the checker refuses, and the documentation never
        # said which ones exist — the model has no way to get it right.
        sources.append((name[len('sources."'):-1], lst("args"), lst("answers"), lst("aggregates")))
    elif name.startswith("kinds."):
        kinds.append((name[len("kinds."):], args))
    else:
        roles.append((name, args))

# A token set is joined with " | ", and a bare pipe inside a table cell OPENS A
# COLUMN. `Map`'s row rendered as five columns of a two-column table, and the
# `unit` set as ten. The pipes have to be escaped where the cell is written,
# not where the set is joined — the joined form is also read as plain text.
def cell(x):
    return x.replace("|", r"\|")


out += ["## Roles", "", "| role | arguments |", "|---|---|"]
for n, a in sorted(roles):
    out.append(f"| `{n}` | {', '.join(f'`{cell(x)}`' for x in a) if a else '—'} |")
out += ["", "## Capabilities", "",
        "A `source` may name one of these and nothing else.", "",
        "**`answers` is the whole vocabulary.** A `fields:` list may name only",
        "these, and a view may read only what its source asked for — both are",
        "refused otherwise, because a field nobody can answer renders as missing",
        "and that looks exactly like data still arriving.", "",
        "| capability | arguments | answers |", "|---|---|---|"]
for n, a, ans, agg in sorted(sources):
    cells = ', '.join(f'`{x}`' for x in ans) if ans else '— (not a record)'
    if agg:
        cells += "<br>`aggregate:` " + ', '.join(f'`{x}`' for x in agg)
    out.append(f"| `{n}` | {', '.join(f'`{cell(x)}`' for x in a) if a else '— (no arguments)'} | {cells} |")
if kinds:
    out += ["", "## Shared token sets", "", "| set | tokens |", "|---|---|"]
    for n, a in sorted(kinds):
        vals = next((x.split(": ", 1)[1] for x in a if x.startswith("tokens")), "")
        out.append(f"| `{n}` | {cell(vals)} |")
print("\n".join(out))
