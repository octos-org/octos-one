#!/usr/bin/env bash
# Card control-flow and imperative-call counts, for docs/LEDGER-ARCHITECTURE.md §2.1/§4.5.
#
# Comments MUST be stripped first. Three separate errors in revisions R4-R7 of that
# document came from grepping raw source: `while` matched "while the fetch is loading",
# `for` matched "results for the typed query". The counts below are the corrected ones.
#
# The `ui.` pattern must also admit digits and capitals in widget ids — an earlier
# `ui\.[a-z_]+\.[a-z_]+` undercounted trip-planner's imperative calls by 28.
#
# Usage:  docs/tools/card-metrics.sh [file...]     (defaults to the nav cards)

set -euo pipefail
cd "$(dirname "$0")/../.."

files=("$@")
if [ ${#files[@]} -eq 0 ]; then
  files=(a2app/apps/nav/exemplars/trip-planner.splash a2app/apps/nav/cards/navigate.splash)
fi

strip_comments() { sed 's|//.*||' "$1"; }
count() { grep -oE "$2" | wc -l | tr -d ' '; }

printf '%-46s %5s %5s %7s %4s %9s %9s\n' FILE if for while fn set_text ui_total
for f in "${files[@]}"; do
  s=$(strip_comments "$f")
  printf '%-46s %5s %5s %7s %4s %9s %9s\n' "$f" \
    "$(echo "$s" | count - '\bif\b')" \
    "$(echo "$s" | count - '\bfor\b')" \
    "$(echo "$s" | count - '\bwhile\b')" \
    "$(echo "$s" | count - '\bfn\b')" \
    "$(echo "$s" | count - 'ui\.[A-Za-z0-9_]+\.set_text')" \
    "$(echo "$s" | count - 'ui\.[A-Za-z0-9_]+\.[a-z_]+')"
done

echo
echo "imperative call breakdown:"
for f in "${files[@]}"; do
  echo "  $f"
  strip_comments "$f" \
    | grep -oE 'ui\.[A-Za-z0-9_]+\.[a-z_]+' \
    | sed 's/ui\.[A-Za-z0-9_]*\./  ui.*./' \
    | sort | uniq -c | sort -rn
done
