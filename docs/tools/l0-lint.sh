#!/usr/bin/env bash
# Check that a card claiming Level 0 actually stays inside the profile.
#
# This is a NEGATIVE lint, not a parser: it proves the two properties L0 is
# supposed to buy — no authority-bearing operation, and no expression — by
# showing the constructs that would grant them are absent. Grammar membership
# needs the real parser in splash-core (docs/ui-profile-l0.md §2); this is what
# can be checked before that exists.
#
# Usage:  docs/tools/l0-lint.sh [card...]     (defaults to docs/l0/*.card)

set -uo pipefail
cd "$(dirname "$0")/../.."

cards=("$@")
[ ${#cards[@]} -eq 0 ] && cards=(docs/l0/*.card)

# Parallel arrays, because the regexes themselves contain `|`.
names=(
  "module access" "imperative widget call" "function definition" "unbounded loop"
  "arithmetic" "ternary" "negation in transition" "string concatenation"
)
regexes=(
  '(^|[^a-z])mod\.'
  'ui\.[A-Za-z0-9_]+\.'
  '(^|[[:space:]])fn[[:space:]]'
  '(^|[[:space:]])while[[:space:]]'
  '[a-z0-9_)][[:space:]]*[*/%+][[:space:]]*[a-z0-9_(]'
  '\?[^"]*:'
  ':[[:space:]]*![a-z]'
  '"[[:space:]]*\+'
)
whys=(
  "reaches a capability; L0's host surface is empty"
  "commands are L2, not declarations"
  "reintroduces unbounded work"
  "iteration must be over a declared collection"
  "expressions can hide fabricated facts"
  "transitions must use a total form (set/toggle/cycle/clear)"
  "!x is computation; use toggle"
  'concatenating onto a live value is how "34 mph" shipped'
)

fail=0
for card in "${cards[@]}"; do
  [ -f "$card" ] || { echo "missing: $card"; fail=1; continue; }
  # Strip BOTH comment syntaxes before scanning: `#` for .card headers and
  # `//` for Splash source. Missing the second is exactly how three counts in
  # docs/LEDGER-ARCHITECTURE.md came out wrong across four revisions — "while
  # the fetch is loading" counted as a loop. Never scan raw source.
  body=$(sed -e 's|//.*||' -e 's|#.*||' "$card")
  printf '%s\n' "$card"
  clean=1
  for i in "${!names[@]}"; do
    name="${names[$i]}"; re="${regexes[$i]}"; why="${whys[$i]}"
    if hits=$(printf '%s\n' "$body" | grep -nE "$re"); then
      clean=0; fail=1
      printf '  FAIL  %-24s %s\n' "$name" "$why"
      printf '%s\n' "$hits" | sed 's/^/          /' | head -3
    fi
  done
  [ $clean -eq 1 ] && printf '  ok    no authority-bearing operation, no expression\n'
done

echo
[ $fail -eq 0 ] && echo "all cards stay inside L0" || echo "L0 violations found"
exit $fail
