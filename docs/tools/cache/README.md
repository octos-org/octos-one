# Cache measurement scripts

The scripts behind §9 of `docs/LEDGER-ARCHITECTURE.md`. They were quoted there
for two revisions while living only in a session scratchpad, which made every
number in that section unreproducible — the failure the section itself warns
about when it says byte counting is not a cache test.

## What they measure

| Script | Question |
|---|---|
| `cache_experiment.py` | Does appending preserve the cache, and does editing destroy it? (OpenAI) |
| `glm_msgs.py` | The same, on GLM-5.2 via z.ai's Anthropic-compatible endpoint |
| `kimi_msgs.py` | The same, on Kimi K3 via Moonshot |
| `latency.py` | Is the saving prefill or decode? TTFT and total, cold vs warm |

## Running them

```
OPENAI_API_KEY=…    python3 cache_experiment.py openai
ZAI_API_KEY=…       python3 glm_msgs.py
MOONSHOT_API_KEY=…  python3 kimi_msgs.py
OPENAI_API_KEY=…    python3 latency.py
```

Each reports the provider's own usage fields — `cache_read_input_tokens` or
`cached_tokens` — never an inference from wall-clock time.

## What the numbers are, and are not

Cached-token **fractions** for one prompt shape, from single runs, with no
repetitions, confidence intervals, TTL-expiry tests or concurrent cold starts.
They demonstrate a **mechanism** — a stable prefix caches, mutation destroys it,
the saving falls on prefill — not a hit rate under load.

## The result that matters

Append as a new message; never grow an existing one. Measured 96.9–99.0% versus
**0%** for a one-word edit early in the same document.

An earlier revision of the architecture doc concluded the opposite, from a test
that grew a message instead of appending one. If you change these scripts, check
which of the two you are actually doing.
