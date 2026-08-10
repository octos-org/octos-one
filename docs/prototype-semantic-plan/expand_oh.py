#!/usr/bin/env python3
"""
Lower the SAME semantic plan to the OpenHarmony / ArkUI backend.

The portability test. `expand.py` lowers plan-kyoto.json to makepad Splash DSL;
this lowers the IDENTICAL plan to the OH dialect, which is different in every
respect that matters:

  makepad                         OpenHarmony (Splash-OH)
  -----------------------------   ------------------------------------------
  Label{...} RoundedView{...}     {t:"text"} {t:"column"} — plain data records
  draw_text.color: #ffffffe6      color: argb(230,255,255,255) — hex is 0 here
  width/height/padding            w/h/pad
  sys.weather(lat, lon, "path")   fetch_fmt(URL, "path", idx, unit)
  sys.geocodenum(place, "lat")    (nothing — resolved at LOWERING time, below)
  MPSL shader widgets             no equivalent; see DEGRADATIONS

So: if one plan can drive both, the plan is genuinely backend-agnostic and the
per-backend cost is confined to a lowering table. That is the whole claim.

DEGRADATIONS — recorded, not hidden. ArkUI has no shader surface, and nobody has
written CPU equivalents, so three blocks cannot render at parity:

  WeatherIcon      NATIVE  — bundled SVGs already exist in rawfile/weather/
  TempBar          DEGRADED to a solid bar coloured at the day's midpoint
  AirQualityField  DEGRADED to a colour band + the AQI number (no contour)
  SunMoon          DEGRADED to times + a linear progress bar + phase as text
  Details          NATIVE
  CurrentConditions NATIVE

Every degradation is announced on stderr, so a silent downgrade is impossible —
the failure mode a generated-UI system must never have.

Usage: expand_oh.py <plan.json> <blocks.json> > card.splash
"""
import json, sys, urllib.request, urllib.parse

DEGRADED = []

def note(block, why):
    DEGRADED.append((block, why))

# ------------------------------------------------------- place resolution

def geocode(place):
    """Resolve a PlaceRef to coordinates AT LOWERING TIME.

    The makepad backend defers this to render time via sys.geocodenum. OH has no
    such helper, so the expander does it here. Either way the PLAN only ever
    names a city — which is the point. Framework code is allowed to look things
    up; the model is not allowed to remember them.
    """
    url = ("https://geocoding-api.open-meteo.com/v1/search?name="
           + urllib.parse.quote(place) + "&count=1&language=en&format=json")
    with urllib.request.urlopen(url, timeout=20) as r:
        d = json.load(r)
    g = d["results"][0]
    return g["latitude"], g["longitude"], g["name"], g.get("timezone", "auto")

# --------------------------------------------------------------- theme

TH = {
    "W": 402, "H": 1700, "PANELW": 370, "PANW_IN": 338, "GAP": 10, "TILEW": 164,
    "type": {
        "city":      (30, 4), "hero": (76, 2), "condition": (18, 4),
        "stat":      (14, 4), "row":  (15, 5), "row_dim": (15, 4),
        "caption":   (12, 5), "tile_cap": (12, 5), "tile_val": (18, 6),
    },
    "color": {
        "white": (255, 255, 255, 255), "light": (236, 235, 239, 247),
        "dim":   (196, 209, 216, 230), "label": (222, 223, 229, 242),
        "scrim": (120, 8, 10, 16),     "panel": (165, 19, 23, 33),
        "tile":  (80, 140, 151, 178),  "track": (60, 255, 255, 255),
    },
    "icons": {"clear": "sunny", "partly_cloudy": "partly", "cloudy": "cloudy",
              "rain": "rain", "thunderstorm": "storm", "snow": "cloudy",
              "wind": "mostly", "fog": "fog"},
    # AQI band -> argb, EPA categories
    "aqi_bands": [(50, (200, 0, 228, 0)), (100, (200, 255, 255, 0)),
                  (150, (200, 255, 126, 0)), (200, (200, 255, 0, 0)),
                  (300, (200, 143, 63, 151)), (10**9, (200, 126, 0, 35))],
    "strings": {
        "en": {"air_quality": "Air Quality", "sun": "Sunrise / Sunset",
               "illuminated": "% illuminated", "aqi": "AIR QUALITY",
               "uv": "UV INDEX", "humidity": "HUMIDITY", "wind": "WIND",
               "days": ["Today", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]},
        "zh": {"air_quality": "空气质量", "sun": "日出 / 日落",
               "illuminated": "% 照亮", "aqi": "空气质量",
               "uv": "紫外线", "humidity": "湿度", "wind": "风速",
               "days": ["今天", "周一", "周二", "周三", "周四", "周五", "周六", "周日"]},
    },
}

def C(name):
    a, r, g, b = TH["color"][name]
    return f"argb({a}, {r}, {g}, {b})"

def argb_lit(t):
    a, r, g, b = t
    return f"argb({a}, {r}, {g}, {b})"

# --------------------------------------------------------------- blocks

class Ctx:
    def __init__(self, plan):
        self.locale = plan["locale"]
        self.S = TH["strings"][self.locale]
        self.lat, self.lon, self.name, self.tz = geocode(plan["place"]["query"])
        tzq = urllib.parse.quote(self.tz)
        self.WX = (f"https://api.open-meteo.com/v1/forecast?latitude={self.lat:.4f}"
                   f"&longitude={self.lon:.4f}&current=temperature_2m,relative_humidity_2m,"
                   f"apparent_temperature,weather_code,wind_speed_10m&daily=weather_code,"
                   f"temperature_2m_max,temperature_2m_min,uv_index_max,sunrise,sunset"
                   f"&timezone={tzq}&forecast_days=7")
        self.AQ = (f"https://air-quality-api.open-meteo.com/v1/air-quality?latitude={self.lat:.4f}"
                   f"&longitude={self.lon:.4f}&current=us_aqi&timezone={tzq}")

    def txt(self, role, expr, w=None, color="white"):
        size, weight = TH["type"][role]
        w = w if w is not None else TH["PANW_IN"]
        return (f'{{t: "text", text: {expr}, size: {size}, weight: {weight}, '
                f'color: {C(color)}, w: {w}, h: {size} * 1.3 + 8}}')

def current_conditions(ctx, args):
    icon = TH["icons"][args["condition"]]
    word = args["condition"].replace("_", " ").title() if ctx.locale == "en" else \
        {"clear": "晴", "partly_cloudy": "局部多云", "cloudy": "多云", "rain": "雨",
         "thunderstorm": "雷暴", "snow": "雪", "wind": "大风", "fog": "雾"}[args["condition"]]
    return f'''    {{t: "column", w: PANELW, align: 1, c: [
        {ctx.txt("city", json.dumps(ctx.name), TH["PANELW"], "light")},
        {ctx.txt("hero", f'fetch_fmt(WX, "current.temperature_2m", -1, "°")', TH["PANELW"])},
        {{t: "row", w: PANELW, h: 60, align: 1, c: [
            {{t: "image", src: "resource://RAWFILE/weather/{icon}.svg", w: 46, h: 46, fit: 1}},
            {{t: "column", w: 8, h: 4}},
            {ctx.txt("condition", json.dumps(word, ensure_ascii=False), 200, "light")}
        ]}},
        {ctx.txt("stat",
                 '"↑" + fetch_fmt(WX, "daily.temperature_2m_max", 0, "°") + "   ↓" + '
                 'fetch_fmt(WX, "daily.temperature_2m_min", 0, "°") + "   ≈" + '
                 'fetch_fmt(WX, "current.apparent_temperature", -1, "°")',
                 TH["PANELW"], "dim")}
    ]}}'''

def forecast(ctx, args):
    note("TempBar", "ArkUI has no shader surface; drawn as a SOLID bar coloured at "
                    "the day's midpoint instead of a cool->warm gradient")
    days = args.get("days", 7)
    conds = args.get("conditions") or ["cloudy"] * days
    rows = []
    for i in range(days):
        icon = TH["icons"][conds[i]]
        # Degraded bar: a track plus a solid segment. A gradient would need a
        # shader or a per-pixel bitmap, neither of which this backend exposes.
        bar = (f'{{t: "stack", w: 90, h: 6, c: [\n'
               f'                {{t: "column", w: 90, h: 6, bg: {C("track")}, radius: 3}},\n'
               f'                {{t: "column", w: 90, h: 6, bg: bar_color(fetch_num(WX, "daily.temperature_2m_max", {i})), radius: 3}}\n'
               f'            ]}}')
        rows.append(f'''        {{t: "row", w: PANW_IN, h: 44, align: 1, c: [
            {ctx.txt("row", (f'"{ctx.S["days"][0]}"' if i == 0 else f'fetch_weekday(WX, "daily.time", {i})'), 86, "light")},
            {{t: "image", src: "resource://RAWFILE/weather/{icon}.svg", w: 26, h: 26, fit: 1}},
            {{t: "column", w: 8, h: 4}},
            {ctx.txt("row_dim", f'fetch_fmt(WX, "daily.temperature_2m_min", {i}, "°")', 46, "dim")},
            {bar},
            {{t: "column", w: 8, h: 4}},
            {ctx.txt("row", f'fetch_fmt(WX, "daily.temperature_2m_max", {i}, "°")', 46)}
        ]}}''')
    return (f'    {{t: "column", w: PANELW, bg: {C("panel")}, radius: 20, pad: 16, c: [\n'
            + ",\n".join(rows) + "\n    ]}")

def air_quality_field(ctx, args):
    note("AirQualityField", "no contour surface and no 4x4 sampling; drawn as a "
                            "single EPA-coloured band plus the AQI number")
    return f'''    {{t: "column", w: PANELW, bg: {C("panel")}, radius: 20, pad: 16, c: [
        {ctx.txt("caption", json.dumps(ctx.S["air_quality"], ensure_ascii=False), TH["PANW_IN"], "dim")},
        {{t: "column", w: 4, h: 8}},
        {{t: "column", w: PANW_IN, h: 26, bg: aqi_color(fetch_num(AQ, "current.us_aqi", -1)), radius: 13}},
        {{t: "column", w: 4, h: 8}},
        {ctx.txt("tile_val", 'fetch_fmt(AQ, "current.us_aqi", -1, "")', TH["PANW_IN"])}
    ]}}'''

def sun_moon(ctx, args):
    """UNSUPPORTED on this backend — and it says so, on screen.

    Not a shader problem. This backend injects exactly five host functions
    (fetch_num, fetch_fmt, fetch_weekday, invoke, sget) against makepad's thirty-
    odd sys.* helpers, and NONE of them yields a moon phase, an illuminated
    fraction, or a daylight fraction. There is no honest way to render this block
    here, so it renders an explicit unsupported surface rather than being dropped
    or faked. Silently omitting a requested section is the failure mode a
    generated-UI system must never have — the card would look complete and be
    missing a feature nobody could see was missing.
    """
    note("SunMoon", "UNSUPPORTED — this backend has no moon-phase or daylight host "
                    "function; renders an explicit notice. Needs the OH equivalents "
                    "of sys.moonphase / sys.moonnum / sys.daylight.")
    msg = ("Sun & Moon unavailable on this backend"
           if ctx.locale == "en" else "此后端暂不支持日月信息")
    return f'''    {{t: "column", w: PANELW, bg: {C("panel")}, radius: 20, pad: 16, c: [
        {ctx.txt("caption", json.dumps(ctx.S["sun"], ensure_ascii=False), TH["PANW_IN"], "dim")},
        {{t: "column", w: 4, h: 6}},
        {ctx.txt("row_dim", json.dumps(msg, ensure_ascii=False), TH["PANW_IN"], "dim")}
    ]}}'''

TILES = {"aqi": ("AQ", "current.us_aqi", ""), "uv": ("WX", "daily.uv_index_max", ""),
         "humidity": ("WX", "current.relative_humidity_2m", "%"),
         "wind": ("WX", "current.wind_speed_10m", " km/h")}

def details(ctx, args):
    cells = []
    for key in args["tiles"]:
        src, path, unit = TILES[key]
        idx = "0" if path.startswith("daily") else "-1"
        cells.append(f'''        {{t: "column", w: TILEW, h: 80, bg: {C("tile")}, radius: 14, pad: 12, c: [
            {ctx.txt("tile_cap", json.dumps(ctx.S[key], ensure_ascii=False), TH["TILEW"] - 24, "dim")},
            {{t: "column", w: 4, h: 4}},
            {ctx.txt("tile_val", f'fetch_fmt({src}, "{path}", {idx}, "{unit}")', TH["TILEW"] - 24)}
        ]}}''')
    rows = []
    for i in range(0, len(cells), 2):
        pair = cells[i:i + 2]
        inner = (",\n            {t: \"column\", w: GAP, h: 4},\n".join(pair))
        rows.append(f'    {{t: "row", w: PANELW, h: 92, c: [\n{inner}\n    ]}}')
    return ",\n".join(rows)

BLOCKS = {"CurrentConditions": current_conditions, "Forecast": forecast,
          "AirQualityField": air_quality_field, "SunMoon": sun_moon, "Details": details}

# ---------------------------------------------------------------- lower

PRELUDE = '''// GENERATED from a semantic plan by expand_oh.py — do not edit.
//
// The SAME plan that expand.py lowers to makepad Splash DSL, lowered here to the
// OpenHarmony dialect: plain {t:...} records, argb() colours (hex evaluates to 0
// in this VM), w/h attributes, and fetch_* against explicit URLs.

fn argb(a, r, g, b) { return ((a * 256 + r) * 256 + g) * 256 + b }
'''

def lower(plan):
    ctx = Ctx(plan)
    body = ",\n".join(BLOCKS[s["block"]](ctx, s.get("args", {})) for s in plan["sections"])
    S = ctx.S
    days = ", ".join(json.dumps(d, ensure_ascii=False) for d in S["days"])
    bands = "\n".join(
        f'    if v <= {hi} {{ return {argb_lit(c)} }}' for hi, c in TH["aqi_bands"])
    return PRELUDE + f'''
let W       = {TH["W"]}
let H       = {TH["H"]}
let PANELW  = {TH["PANELW"]}
let PANW_IN = {TH["PANW_IN"]}
let TILEW   = {TH["TILEW"]}
let GAP     = {TH["GAP"]}

let WX = "{ctx.WX}"
let AQ = "{ctx.AQ}"

// Weekday labels. The plan does not carry them and the model never sees them —
// same rule as sys.dayname on the makepad side.
let DAYS = [{days}]
fn dayname(n) {{
    if n <= 0 {{ return DAYS[0] }}
    return DAYS[(weekday_today() + n - 1) % 7 + 1]
}}

// EPA band colour for an AQI reading.
fn aqi_color(v) {{
{bands}
    return {argb_lit(TH["aqi_bands"][-1][1])}
}}

// DEGRADED stand-in for TempBar's gradient: one colour per row, picked from the
// day's high. A true gradient needs a shader this backend does not have.
fn bar_color(t) {{
    if t <= 10 {{ return argb(220,  30,  92, 255) }}
    if t <= 20 {{ return argb(220,   0, 217, 192) }}
    if t <= 27 {{ return argb(220, 198, 224,  22) }}
    if t <= 32 {{ return argb(220, 255, 196,   0) }}
    if t <= 36 {{ return argb(220, 255, 138,   0) }}
    return argb(220, 224,  27,  27)
}}

{{t: "scroll", w: W, h: H, c: [
  {{t: "column", w: W, bg: {C("scrim")}, pad: 16, align: 1, c: [
{body}
  ]}}
]}}
'''

if __name__ == "__main__":
    plan = json.load(open(sys.argv[1]))
    reg = json.load(open(sys.argv[2]))
    sys.path.insert(0, __import__("os").path.dirname(__file__))
    from expand import validate, Reject          # SAME validator, both backends
    try:
        validate(plan, reg)
    except Reject as e:
        sys.exit(f"PLAN REJECTED: {e}")
    out = lower(plan)
    sys.stdout.write(out)
    for b, why in DEGRADED:
        print(f"DEGRADED  {b}: {why}", file=sys.stderr)
