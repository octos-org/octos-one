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
for m in re.finditer(r'^\[([A-Za-z.\"]+[^\]]*)\]\n((?:[a-z_]+ *=.*\n)*)', src, re.M):
    name, body = m.group(1), m.group(2)
    args = []
    for line in body.strip().splitlines():
        if not line.strip():
            continue
        k, v = line.split("=", 1)
        toks = re.search(r'tokens *= *\[([^\]]*)\]', v)
        kind = re.search(r'kind *= *"([a-z]+)"', v)
        desc = kind.group(1) if kind else "?"
        if toks:
            desc = " | ".join("." + t.strip().strip('"') for t in toks.group(1).split(","))
        args.append(f"{k.strip()}: {desc}")
    if name.startswith("sources."):
        # A source declares `args = ["lat", "lon", …]`, a list — not one entry
        # per argument the way a role does.
        lst = re.search(r'args *= *\[([^\]]*)\]', body)
        names = [a.strip().strip('"') for a in lst.group(1).split(",") if a.strip()] if lst else []
        sources.append((name[len('sources."'):-1], names))
    elif name.startswith("kinds."):
        kinds.append((name[len("kinds."):], args))
    else:
        roles.append((name, args))

out += ["## Roles", "", "| role | arguments |", "|---|---|"]
for n, a in sorted(roles):
    out.append(f"| `{n}` | {', '.join(f'`{x}`' for x in a) if a else '—'} |")
out += ["", "## Capabilities", "",
        "A `source` may name one of these and nothing else.", "",
        "| capability | arguments |", "|---|---|"]
for n, a in sorted(sources):
    out.append(f"| `{n}` | {', '.join(f'`{x}`' for x in a) if a else '— (no arguments)'} |")
if kinds:
    out += ["", "## Shared token sets", "", "| set | tokens |", "|---|---|"]
    for n, a in sorted(kinds):
        vals = next((x.split(": ", 1)[1] for x in a if x.startswith("tokens")), "")
        out.append(f"| `{n}` | {vals} |")
print("\n".join(out))
