#!/usr/bin/env python3
"""
Fulfil an L0 card's source plan with live data.

This is the HOST half of the split. splash-core reads the card and says what it
needs — helper, arguments, and an order that satisfies dependencies. It never
fetches anything, because only a host knows what answers `sys.weather`. That
separation is what lets realization run against an empty host surface.

    resolve-sources.py <card> [city] > data.json

Deliberately small and dependency-free: it exists to prove the plan is
actionable, not to be the production resolver. octos-one already has ~42 `sys.*`
helpers; the real work is declaring them once rather than a fifth time here.
"""

import json
import subprocess
import sys
import urllib.parse
import urllib.request
from pathlib import Path

SPLASH = Path(__file__).resolve().parents[3] / "Splash"


def plan(card_path):
    """Ask splash-core what this card needs, in resolution order."""
    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "splash-core", "--example", "source_plan",
         "--", str(card_path)],
        cwd=SPLASH, capture_output=True, text=True,
    )
    if out.returncode != 0:
        raise SystemExit(f"source_plan failed:\n{out.stderr}")
    return json.loads(out.stdout)


def get(url):
    with urllib.request.urlopen(url, timeout=20) as r:
        return json.loads(r.read())


# One handler per helper. A card names the helper; the host decides what answers
# it — and can answer differently on a phone, a desktop or a test.
def sys_geocode(args, seen):
    name = resolve(args.get("name"), seen) or "Kyoto"
    q = urllib.parse.quote(str(name))
    hit = get(f"https://geocoding-api.open-meteo.com/v1/search?name={q}&count=1")
    r = (hit.get("results") or [{}])[0]
    return {"name": r.get("name", name), "lat": r.get("latitude", 0.0),
            "lon": r.get("longitude", 0.0)}


def sys_weather(args, seen):
    lat, lon = resolve(args.get("lat"), seen), resolve(args.get("lon"), seen)
    days = int(resolve(args.get("days"), seen) or 7)
    url = (f"https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}"
           "&current=temperature_2m,apparent_temperature,relative_humidity_2m,"
           "surface_pressure,wind_speed_10m,weather_code"
           "&daily=temperature_2m_max,temperature_2m_min,weather_code"
           f"&forecast_days={days}&timezone=auto")
    w = get(url)
    cur, daily = w.get("current", {}), w.get("daily", {})

    # A `days` argument means the card wants the week; without it, today.
    if "days" in args:
        rows = []
        names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        for i, date in enumerate(daily.get("time", [])):
            y, m, d = (int(x) for x in date.split("-"))
            # Zeller's congruence — a weekday from a date, no dependency.
            yy, mm = (y - 1, m + 12) if m < 3 else (y, m)
            h = (d + (13 * (mm + 1)) // 5 + yy + yy // 4 - yy // 100 + yy // 400) % 7
            rows.append({
                "dayname": names[(h + 5) % 7],
                "hi": daily["temperature_2m_max"][i],
                "lo": daily["temperature_2m_min"][i],
                "cond": daily["weather_code"][i],
            })
        return {"days": rows,
                "min_lo": min(r["lo"] for r in rows),
                "max_hi": max(r["hi"] for r in rows)}

    return {"temp": cur.get("temperature_2m"), "feels": cur.get("apparent_temperature"),
            "hi": max(daily.get("temperature_2m_max", [0])[:1] or [0]),
            "lo": min(daily.get("temperature_2m_min", [0])[:1] or [0]),
            "cond": cur.get("weather_code"), "humidity": cur.get("relative_humidity_2m"),
            "wind": cur.get("wind_speed_10m"), "pressure": cur.get("surface_pressure"),
            "uv": 0, "visibility": 0}


def sys_daylight(args, seen):
    lat, lon = resolve(args.get("lat"), seen), resolve(args.get("lon"), seen)
    d = get(f"https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}"
            "&daily=sunrise,sunset&forecast_days=1&timezone=auto")["daily"]
    def hour(stamp):
        t = stamp.split("T")[1]
        return int(t[:2]) + int(t[3:5]) / 60
    return {"rise": hour(d["sunrise"][0]), "set": hour(d["sunset"][0]), "now": 12.0}


def sys_airquality(args, seen):
    lat, lon = resolve(args.get("lat"), seen), resolve(args.get("lon"), seen)
    try:
        a = get("https://air-quality-api.open-meteo.com/v1/air-quality"
                f"?latitude={lat}&longitude={lon}&current=us_aqi")["current"]
        idx = a.get("us_aqi") or 0
    except Exception:
        idx = 0
    band = "good" if idx <= 50 else "moderate" if idx <= 100 else "poor"
    return {"grid": [], "index": idx, "band": band}


def sys_photo(args, seen):
    q = urllib.parse.quote(f"{resolve(args.get('query'), seen)} skyline")
    return f"https://image.pollinations.ai/prompt/{q}?width=1080&height=1920&nologo=true"


def sys_moonphase(args, seen):
    return {"phase": 0.5, "illumination": 0.5}


def sys_locale(args, seen):
    return {"lang": "en", "temp_unit": "c"}


HANDLERS = {
    "sys.geocode": sys_geocode, "sys.weather": sys_weather,
    "sys.daylight": sys_daylight, "sys.airquality": sys_airquality,
    "sys.photo": sys_photo, "sys.moonphase": sys_moonphase,
    "sys.locale": sys_locale,
}


def resolve(arg, seen):
    """An argument value, following a path into what has already been fetched."""
    if arg is None:
        return None
    kind, value = arg["kind"], arg["value"]
    if kind != "path":
        return value
    cur = seen
    for seg in value.split("."):
        if not isinstance(cur, dict) or seg not in cur:
            return None
        cur = cur[seg]
    return cur


def main():
    card = Path(sys.argv[1])
    city = sys.argv[2] if len(sys.argv) > 2 else "Kyoto"

    p = plan(card)
    for d in p.get("diagnostics", []):
        print(f"{card}:{d['line']}: {d['message']}", file=sys.stderr)

    # Card state the sources read. A real host holds these; here the city is the
    # one input the card takes.
    seen = {"city": city, "units": "c", "days": 7, "selected": "", "range": "m1"}

    for req in p["requests"]:
        handler = HANDLERS.get(req["helper"])
        if handler is None:
            print(f"  no handler for {req['helper']} ({req['name']})", file=sys.stderr)
            continue
        args = {name: a for name, a in req["args"]}
        try:
            value = handler(args, seen)
        except Exception as e:
            # A source that fails resolves to nothing; the card renders an em
            # dash rather than inventing a value.
            print(f"  {req['name']} failed: {e}", file=sys.stderr)
            value = None
        # `env.locale` binds a dotted name.
        target, *rest = req["name"].split(".")
        if rest:
            seen.setdefault(target, {})[rest[0]] = value
        else:
            seen[target] = value
        print(f"  resolved {req['name']:<12} via {req['helper']}", file=sys.stderr)

    json.dump(seen, sys.stdout, indent=2)


if __name__ == "__main__":
    main()
