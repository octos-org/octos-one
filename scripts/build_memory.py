#!/usr/bin/env python3
"""Assemble the a2app tree into the single MEMORY.md that octos injects into the
app agents' context every turn (see docs/ARCHITECTURE.md → "The a2app memory").

Order matters: framework first, then the widget helper docs, then one block per
app (its app.md spec + its known-good exemplar). Each file is delimited by a
`===== <relpath> =====` marker so the LLM can tell the sections apart.

Usage:
    python3 scripts/build_memory.py            # writes MEMORY.md next to a2app/
    python3 scripts/build_memory.py --check     # print size + section list only
"""
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
A2APP = os.path.join(ROOT, "a2app")

HEADER = "# APP AGENT MEMORY — your complete manual (use directly; do NOT read files)\n"

# (relpath under a2app/, marker suffix). Add a new app's two lines here when you
# add an app card (see docs/ADDING-AN-APP-CARD.md).
FILES = [
    ("framework.md", ""),
    ("widgets/sys-helpers.md", ""),
    ("widgets/weather-icon.md", ""),
    ("widgets/containers.md", ""),
    ("apps/weather/app.md", ""),
    ("apps/weather/exemplars/weather-canonical.splash", " (known-good reference)"),
    ("apps/stock/app.md", ""),
    ("apps/stock/exemplars/stock-movers.splash", " (known-good reference)"),
    ("apps/stock/exemplars/stock-canonical.splash", " (known-good reference)"),
    ("apps/news/app.md", ""),
    ("apps/news/exemplars/news-canonical.splash", " (known-good reference)"),
    ("apps/web/app.md", ""),
    ("apps/youtube/app.md", ""),
]


def build() -> str:
    out = HEADER
    for rel, suffix in FILES:
        with open(os.path.join(A2APP, rel), encoding="utf-8") as f:
            body = f.read()
        if not body.endswith("\n"):
            body += "\n"
        out += f"\n===== {rel}{suffix} =====\n{body}"
    return out


def main() -> None:
    out = build()
    nbytes = len(out.encode("utf-8"))
    approx_tokens = nbytes // 4
    if "--check" in sys.argv:
        print(f"MEMORY.md would be {nbytes} bytes (~{approx_tokens} tokens)")
        for rel, _ in FILES:
            print("  -", rel)
        print("NOTE: keep _main.json config.memory.max_inject_tokens ABOVE the token")
        print("estimate, or octos truncates the tail (the last app) on injection.")
        return
    dest = os.path.join(ROOT, "MEMORY.md")
    with open(dest, "w", encoding="utf-8", newline="\n") as f:
        f.write(out)
    print(f"wrote {dest} ({nbytes} bytes, ~{approx_tokens} tokens)")


if __name__ == "__main__":
    main()
