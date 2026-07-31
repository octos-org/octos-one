#!/usr/bin/env python3
"""
Does an append-only ledger actually hit the inference-side prompt cache?

Four questions, in order of how much they matter to the design:

  E1  Does APPENDING to the tail preserve the cache, while editing the
      middle destroys it?                      <- the central design assumption
  E2  Does a SECOND user hit the cache the first user warmed?
                                               <- the whole app-store model
  E3  Does an explicit cache key improve the hit rate?
                                               <- "app UUID tells inference which cache"
  E4  Anthropic only: does splitting into blocks with the breakpoint at the
      sealed/open boundary beat one monolithic block?

Usage:
    OPENAI_API_KEY=sk-...      python3 cache_experiment.py openai
    ANTHROPIC_API_KEY=sk-ant-. python3 cache_experiment.py anthropic

Costs a few cents. Uses only the stdlib.
"""

import json
import os
import sys
import time
import urllib.request

# Provider caches need a minimum prefix (OpenAI ~1024 tokens) and match in
# chunks, so the baseline has to be comfortably large. It is also written as
# DECLARATIONS rather than prose: code and prose tokenize differently, and a
# result measured on prose would not transfer to a ledger.
BASELINE_RECORDS = 260


def baseline(n=BASELINE_RECORDS):
    """A synthetic app ledger — stands in for the shared, distributed baseline."""
    out = ["# ledger travel-planner@1.0.0", ""]
    for i in range(n):
        out.append(f"@{i:04d} source  place.{i:03d}    sys.geocode(name: city_{i:03d}, fields: [lat, lon])")
        out.append(f"@{i:04d} view    card.{i:03d}     Row(title: place.{i:03d}.name, sub: place.{i:03d}.region)")
    return "\n".join(out)


def personal_delta(user, n=6):
    """A per-user overlay. Small, unique, shadows baseline records."""
    out = [f"# overlay user-{user}"]
    for i in range(n):
        out.append(f"@9{i:03d} view    card.{i:03d}     Row(title: place.{i:03d}.name, emphasis: high)")
    return "\n".join(out)


def post(url, payload, headers):
    req = urllib.request.Request(
        url, data=json.dumps(payload).encode(), headers=headers, method="POST"
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.loads(r.read())


# ----------------------------------------------------------------- OpenAI ---
# Caching is automatic: no breakpoints to place. That makes it the cleanest
# test of the core claim, because there is no block design to get wrong.

def openai_call(key, model, text, cache_key=None):
    payload = {
        "model": model,
        "messages": [{"role": "user", "content": text}],
        "max_completion_tokens": 16,
    }
    if cache_key:
        payload["prompt_cache_key"] = cache_key
    r = post(
        "https://api.openai.com/v1/chat/completions",
        payload,
        {"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
    )
    u = r["usage"]
    cached = u.get("prompt_tokens_details", {}).get("cached_tokens", 0)
    return u["prompt_tokens"], cached


# -------------------------------------------------------------- Anthropic ---
# Caching is explicit. `blocks` is a list of (text, breakpoint?) pairs, so E4
# can compare one monolithic block against a split with the breakpoint at the
# sealed/open boundary.

def anthropic_call(key, model, blocks):
    content = []
    for text, mark in blocks:
        b = {"type": "text", "text": text}
        if mark:
            b["cache_control"] = {"type": "ephemeral"}
        content.append(b)
    r = post(
        "https://api.anthropic.com/v1/messages",
        {
            "model": model,
            "max_tokens": 16,
            "messages": [{"role": "user", "content": content}],
        },
        {
            "x-api-key": key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
    )
    u = r["usage"]
    return (
        u["input_tokens"],
        u.get("cache_read_input_tokens", 0),
        u.get("cache_creation_input_tokens", 0),
    )


def row(label, total, cached, extra=""):
    pct = (100.0 * cached / total) if total else 0.0
    print(f"  {label:<34} {total:>7} in  {cached:>7} cached  {pct:5.1f}%  {extra}")


def run_openai(key, model):
    base = baseline()
    print(f"\nmodel: {model}   baseline: {len(base)} chars\n")

    print("E1  append vs edit-in-middle")
    t, c = openai_call(key, model, base + "\n\nRequest: summarize in 3 words.")
    row("1. cold", t, c)
    time.sleep(2)
    t, c = openai_call(key, model, base + "\n\nRequest: summarize in 3 words.")
    row("2. identical (warm)", t, c, "<- baseline for comparison")
    time.sleep(2)
    t, c = openai_call(
        key, model, base + "\n" + personal_delta("A") + "\n\nRequest: summarize in 3 words."
    )
    row("3. APPEND at tail", t, c, "<- should stay high")
    time.sleep(2)
    edited = base.replace("@0005 source", "@0005 SOURCE", 1)  # one line, early
    t, c = openai_call(key, model, edited + "\n\nRequest: summarize in 3 words.")
    row("4. EDIT in middle", t, c, "<- should collapse")

    print("\nE2  does a second user hit the first user's cache?")
    time.sleep(2)
    t, c = openai_call(
        key, model, base + "\n" + personal_delta("B") + "\n\nRequest: summarize in 3 words."
    )
    row("user B, same baseline", t, c, "<- app-store model rests on this")

    print("\nE3  explicit cache key (the 'app UUID' idea)")
    time.sleep(2)
    t, c = openai_call(
        key, model, base + "\n" + personal_delta("C") + "\n\nRequest: go.", cache_key="travel-planner@1.0.0"
    )
    row("user C, with cache key", t, c)


def run_anthropic(key, model):
    base = baseline()
    print(f"\nmodel: {model}   baseline: {len(base)} chars\n")

    print("E4  monolithic block vs split with breakpoint at sealed/open boundary")
    # Monolithic: everything in one block, breakpoint at the end. This is what
    # octos-one does today (Message.content is one String).
    mono = base + "\n" + personal_delta("A") + "\n\nRequest: summarize in 3 words."
    t, r_, w = anthropic_call(key, model, [(mono, True)])
    row("1. monolithic, cold", t, r_, f"write={w}")
    time.sleep(2)
    mono2 = base + "\n" + personal_delta("A") + personal_delta("A2") + "\n\nRequest: go."
    t, r_, w = anthropic_call(key, model, [(mono2, True)])
    row("2. monolithic + append", t, r_, f"write={w}  <- expect ~0 read")

    time.sleep(2)
    # Split: breakpoint AFTER the stable baseline, appends land in a later block.
    t, r_, w = anthropic_call(
        key, model,
        [(base, True), (personal_delta("A"), False), ("\n\nRequest: summarize in 3 words.", False)],
    )
    row("3. split, cold", t, r_, f"write={w}")
    time.sleep(2)
    t, r_, w = anthropic_call(
        key, model,
        [(base, True), (personal_delta("A") + personal_delta("A2"), False), ("\n\nRequest: go.", False)],
    )
    row("4. split + append", t, r_, f"write={w}  <- expect high read")

    print("\nE2  second user, same baseline block")
    time.sleep(2)
    t, r_, w = anthropic_call(
        key, model,
        [(base, True), (personal_delta("B"), False), ("\n\nRequest: go.", False)],
    )
    row("user B", t, r_, f"write={w}")


def main():
    which = sys.argv[1] if len(sys.argv) > 1 else "openai"
    if which == "openai":
        key = os.environ.get("OPENAI_API_KEY")
        if not key:
            sys.exit("set OPENAI_API_KEY")
        run_openai(key, os.environ.get("MODEL", "gpt-4o-mini"))
    elif which == "anthropic":
        key = os.environ.get("ANTHROPIC_API_KEY")
        if not key:
            sys.exit("set ANTHROPIC_API_KEY")
        run_anthropic(key, os.environ.get("MODEL", "claude-3-5-haiku-20241022"))
    else:
        sys.exit("usage: cache_experiment.py [openai|anthropic]")


if __name__ == "__main__":
    main()
