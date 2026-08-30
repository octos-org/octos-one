#!/usr/bin/env python3
"""Synthesise a data snapshot for any L0 card, from its own source declarations.

Realization needs data keyed by the card's source names, and those names are
card-local — `nvda`, `lead`, `out2`. A hand-written blob per family does not
scale past a handful of cards, and a corpus of one card measures nothing: the
first run of this experiment realized 50 different weather cards to 50
BYTE-IDENTICAL trees, because that corpus varies theme and not structure.

So read the declarations. `fields: [...]` says exactly which keys a card reads,
`for x in <name>` says which sources are collections, and `aggregate: [...]`
adds the bounds a collection is scaled against.

Text lengths are chosen to be realistic, not minimal — the geometry under test
is driven by how much text there is.
"""
import json
import re
import sys

# Field name -> a value of the right type and a believable length. Anything not
# listed falls back to the numeric/string guess in `_value`.
TEXT = {
    "name": "Kyoto", "place": "12 km NW of La Romana", "label": "Kyoto, Japan",
    "title": "Markets steady as central banks hold rates", "dayname": "Mon",
    "cond": "cloudy", "band": "good", "ago": "2h ago", "id": "id-1",
    "summary": "A short standfirst that runs to about this length in a card.",
    "source": "Reuters", "author": "A. Reporter", "channel": "Channel Name",
    "duration": "12 min", "distance": "3.4 km", "step": "Turn left onto Main St",
    "text": "Turn left onto Main Street and continue for 400 metres",
    "query": "kyoto", "ticker": "NVDA", "unit": "c", "temp_unit": "c",
    "lang": "en", "url": "https://example.invalid/x", "views": "1.2M views",
    "thumb": "https://example.invalid/t.jpg",
}
NUM = {
    "temp": 18.0, "feels": 16.0, "hi": 23.0, "lo": 12.0, "mag": 3.9,
    "humidity": 61.0, "wind": 9.0, "pressure": 1013.0, "uv": 3.0,
    "visibility": 10.0, "precip": 12.0, "index": 42.0, "depth": 10.0,
    "lat": 35.0, "lon": 135.8, "rise": 5.1, "set": 18.9, "now": 12.0,
    "phase": 0.5, "illumination": 0.5, "last": 121.4, "change": 2.3,
    "pct": 1.9, "open": 119.0, "high": 122.5, "low": 118.2,
    "volume": 4.2, "mktcap": 3.1, "pe": 54.0, "min_lo": 11.0, "max_hi": 24.0,
    "has": 1.0, "count": 3.0, "days": 7.0,
}


DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


def _value(field, i=0):
    # A loop keys on a field, and duplicate keys are rejected — two rows would
    # share one state cell. So every record's text has to differ.
    if field == "dayname":
        return DAYS[i % 7]
    if field in TEXT:
        v = TEXT[field]
        return f"{v} {i+1}" if i else v
    if field in NUM:
        return NUM[field] + i
    # unknown: a name-shaped field is text, everything else a number
    if re.search(r"name|title|label|text|desc|cond|band|day|time|url|img|thumb|id$", field):
        return f"{field}-{i+1}"
    return 20.0 + i


def synth(card):
    """A data blob for one card, from its `source` lines."""
    # which sources are iterated, and under which key
    lists, keyed = set(), {}
    for m in re.finditer(r"for\s+[a-z_]+(?:\s*,\s*[a-z_]+)?\s+in\s+([a-z0-9_]+)(?:\.([a-z0-9_]+))?", card):
        if m.group(2):
            keyed.setdefault(m.group(1), set()).add(m.group(2))
        else:
            lists.add(m.group(1))

    data = {"env": {"locale": {"temp_unit": "c", "lang": "en", "region": "JP"}}}
    for m in re.finditer(r"^\s*source\s+([a-z0-9_.]+)\s+sys\.([a-z_]+)\(([^\n]*(?:\n\s{2,}[^\n]*)*)\)",
                         card, re.M):
        name, fn, args = m.group(1), m.group(2), m.group(3)
        if name.startswith("env."):
            continue
        fields = re.search(r"fields:\s*\[([^\]]*)\]", args)
        fields = [f.strip() for f in fields.group(1).split(",")] if fields else []
        agg = re.search(r"aggregate:\s*\[([^\]]*)\]", args)
        agg = [f.strip() for f in agg.group(1).split(",")] if agg else []

        if fn == "photo" or fn == "satellite":
            data[name] = "https://example.invalid/scene.jpg"
            continue
        if fn == "locale":
            continue
        if not fields:
            fields = ["name", "label"]

        def rec(i):
            return {f: _value(f, i) for f in fields}

        n = 5
        if name in keyed:
            data[name] = {k: [rec(i) for i in range(n)] for k in keyed[name]}
            for a in agg:
                data[name][a] = NUM.get(a, 20.0)
        elif name in lists:
            data[name] = [rec(i) for i in range(n)]
        else:
            data[name] = rec(0)
            for a in agg:
                data[name][a] = NUM.get(a, 20.0)

    # state initials the card reads back as scalars
    for m in re.finditer(r"^\s*state\s+([a-z0-9_]+)\s*\{[^}]*initial:\s*([^,}\n]+)", card, re.M):
        k, v = m.group(1), m.group(2).strip()
        if k in data:
            continue
        if v.startswith('"'):
            data[k] = v.strip('"')
        else:
            try:
                data[k] = float(v)
            except ValueError:
                pass
    return data


if __name__ == "__main__":
    print(json.dumps(synth(open(sys.argv[1]).read()), indent=1, ensure_ascii=False))
