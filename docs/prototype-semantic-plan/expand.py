#!/usr/bin/env python3
"""
Lower a semantic card PLAN to Splash DSL.

The proof-or-kill prototype for "LLM emits intent, runtime emits the card". Two
claims it is meant to test:

  1. The lowering needs NO intelligence. It is validation + table lookup +
     string building. There is not one heuristic or judgement call below.
  2. Everything the LLM got wrong in the 2026-07-29 session is a decision this
     file makes, so a plan is structurally incapable of getting it wrong:
       - coordinates          -> sys.geocodenum from a PlaceRef (never digits)
       - week extent          -> sys.weekmin / sys.weekmax (never a guess)
       - root height / scroll -> theme.layout (never `height: 858`)
       - one top-level node   -> emitted by construction (no stray sibling)
       - font chain incl. CJK -> theme.fonts (never a tofu box)
       - temp label centring  -> theme.layout (align + inner padding)
       - AQI 16 uniforms      -> generated here (never in the plan)
       - day names            -> theme.strings[locale] (a calendar, not a model)

Usage: expand.py <plan.json> <blocks.json> <theme.json> > card.splash
"""
import json, sys

# ---------------------------------------------------------------- validation

class Reject(Exception):
    """A plan violation. Fails BEFORE lowering, so it can never render broken."""

def sections_of(plan):
    """Every section in a plan, whether it uses top-level `sections` (one screen)
    or `views` (several, chosen by state). Weather needed only the first shape;
    stocks forced the second."""
    if "sections" in plan:
        return list(plan["sections"])
    out = []
    for v in plan.get("views", []):
        out.extend(v.get("sections", []))
    return out


def validate(plan, reg):
    """Reject a plan BEFORE lowering, so it can never render broken.

    Generic across plan kinds: the required top-level shape and the permitted
    block set both come from `reg["plans"][kind]`. The first version hardcoded
    weather's shape and rejected a valid stocks plan, which is what prompted
    declaring the shape rather than assuming it.
    """
    if plan.get("schema") != reg["schema"]:
        raise Reject(f"schema mismatch: plan {plan.get('schema')!r} vs registry {reg['schema']!r}")

    kind = plan.get("plan")
    spec = reg.get("plans", {}).get(kind)
    if spec is None:
        raise Reject(f"unknown plan kind {kind!r}")
    for field in spec["requires"]:
        if field not in plan:
            raise Reject(f"{kind} plan requires {field!r}")

    if plan.get("locale") not in reg["types"]["Locale"]["enum"]:
        raise Reject(f"unsupported locale {plan.get('locale')!r}")

    # A place is a NAME, never coordinates — for any plan kind that has one.
    if "place" in plan:
        place = plan["place"] or {}
        if not ("query" in place or "id" in place):
            raise Reject("place must be a PlaceRef with 'query' or 'id'")
        for k in ("lat", "lon", "latitude", "longitude"):
            if k in place:
                raise Reject(f"place.{k}: PlaceRef carries a NAME, not coordinates")

    conds = reg["types"]["Condition"]["enum"]
    allowed_actions = set(reg.get("actions", {})) - {"_note"}
    for i, sec in enumerate(sections_of(plan)):
        name = sec.get("block")
        if name not in reg["blocks"]:
            raise Reject(f"sections[{i}]: unknown block {name!r}")
        if name not in spec["blocks"]:
            raise Reject(f"sections[{i}]: block {name!r} is not permitted in a {kind} plan")
        bspec = reg["blocks"][name]
        for arg in sec.get("args", {}):
            if arg not in bspec["args"]:
                raise Reject(f"sections[{i}] {name}: unknown arg {arg!r}")
        for arg, aspec in bspec["args"].items():
            if aspec.get("required") and arg not in sec.get("args", {}):
                raise Reject(f"sections[{i}] {name}: missing required arg {arg!r}")
        a = sec.get("args", {})
        if "condition" in a and a["condition"] not in conds:
            raise Reject(f"sections[{i}] {name}: bad condition {a['condition']!r}")
        for c in a.get("conditions", []):
            if c not in conds:
                raise Reject(f"sections[{i}] {name}: bad condition {c!r}")
        # An action must be one the registry DECLARES — a plan cannot invent behaviour.
        if "onTap" in a and a["onTap"] not in allowed_actions:
            raise Reject(f"sections[{i}] {name}: undeclared action {a['onTap']!r}")
        if name == "Details":
            allowed = bspec["args"]["tiles"]["items"]["enum"]
            for t in a.get("tiles", []):
                if t not in allowed:
                    raise Reject(f"sections[{i}] Details: unknown tile {t!r}")


# ------------------------------------------------------------------ helpers

class Ctx:
    def __init__(self, plan, theme):
        self.t = theme
        self.L = theme["layout"]
        self.S = theme["strings"][plan["locale"]]
        self.locale = plan["locale"]
        self.place = plan["place"]["query"]

    def lat(self): return f'sys.geocodenum("{self.place}", "lat")'
    def lon(self): return f'sys.geocodenum("{self.place}", "lon")'
    def ll(self):  return f'{self.lat()}, {self.lon()}'

    def wx(self, path):  return f'sys.weather({self.ll()}, "{path}")'
    def wxn(self, path): return f'sys.weathernum({self.ll()}, "{path}")'

    def dayname(self, i):
        """Weekday label for forecast row `i`, 0 = today.

        THE PROTOTYPE'S MOST INSTRUCTIVE BUG LIVED HERE. Both the LLM-generated
        card and this expander's first version got the sequence WRONG, in
        different ways, and both looked plausible: on Wed 2026-07-29 the correct
        run is Today/Thu/Fri/Sat/Sun/Mon/Tue, the LLM emitted Today/Wed/... (off
        by one, repeating today as tomorrow) and a naive table walk emitted
        Today/Tue/... . Nobody notices, because weekday names are only checkable
        against a calendar nobody consults.

        Worse, a card is PERSISTED in a2app_cards/ and re-served later, so a
        literal weekday name is stale the next day even when it was right when
        generated.

        So in the real runtime this must emit a HELPER CALL, not a string —
        `sys.dayname(N, locale)` resolved at render time off the device clock.
        That helper does not exist yet; below is the literal it would replace,
        computed from the real date so the prototype is at least correct today.
        """
        if i == 0:
            return json.dumps(self.S["days"][0], ensure_ascii=False)
        import datetime
        wd = (datetime.date.today() + datetime.timedelta(days=i)).isoweekday()  # 1=Mon
        return json.dumps(self.S["days"][wd], ensure_ascii=False)

    def font(self, role):
        """Build the font chain from theme tokens.

        ALWAYS includes the CJK member, and the symbol member when the role
        needs arrows: an explicit font_family REPLACES makepad's default chain,
        so omitting either is what produced tofu boxes.
        """
        spec = self.t["type"][role]
        d = self.t["fonts"]["_dir"]
        members = [f'latin   := FontMember{{ res: crate_resource("{d}/{self.t["fonts"][spec["font"]]}") asc: 0.0 desc: 0.0 }}']
        if spec.get("needs_symbols"):
            members.append(f'sym     := FontMember{{ res: crate_resource("{d}/{self.t["fonts"]["symbols"]}") asc: 0.0 desc: 0.0 }}')
        members.append(f'chinese := FontMember{{ res: crate_resource("{d}/{self.t["fonts"]["cjk"]}") asc: 0.0 desc: 0.0 }}')
        return ("draw_text.text_style: TextStyle{ font_family: FontFamily{ "
                + " ".join(members) + f' }} font_size: {spec["size"]} }}')

    def label(self, role, text, extra=""):
        c = self.t["type"][role]["color"]
        return f'Label{{ {extra}text: {text} draw_text.color: {c} {self.font(role)} }}'

# ------------------------------------------------------------------- blocks

def current_conditions(ctx, args):
    icon = ctx.t["condition_icons"][args["condition"]]
    cond_word = args["condition"].replace("_", " ").title() if ctx.locale == "en" else \
                {"clear": "晴", "partly_cloudy": "局部多云", "cloudy": "多云", "rain": "雨",
                 "thunderstorm": "雷暴", "snow": "雪", "wind": "大风", "fog": "雾"}[args["condition"]]
    return f'''        View{{ width: Fill height: Fit flow: Down align: Align{{x: 0.5}}
            {ctx.label("city", f'sys.geocode("{ctx.place}", "name")')}
            {ctx.label("hero", ctx.wx("current.temperature_2m") + ' + "°"', 'margin: Inset{top: 2 bottom: 0} ')}
            View{{ width: Fit height: 52 flow: Right align: Align{{x: 0.5 y: 0.5}} spacing: 8
                WeatherIcon{{ draw_bg.cond: {icon["cond"]} width: 46 height: 46 }}
                {ctx.label("condition", json.dumps(cond_word, ensure_ascii=False))}
            }}
            {ctx.label("stat",
                '"↑" + ' + ctx.wx("daily.temperature_2m_max.0") + ' + "°   ↓" + '
                + ctx.wx("daily.temperature_2m_min.0") + ' + "°   ≈" + '
                + ctx.wx("current.apparent_temperature") + ' + "°"')}
        }}'''

def forecast(ctx, args):
    L, days = ctx.L, args.get("days", 7)
    conds = args.get("conditions") or ["cloudy"] * days
    wmin, wmax = f"sys.weekmin({ctx.ll()})", f"sys.weekmax({ctx.ll()})"
    rows = []
    for i in range(days):
        emoji = ctx.t["condition_icons"][conds[i]]["emoji"]
        rows.append(f'''            View{{ width: Fill height: {L["row_height"]} flow: Right align: Align{{y: 0.5}}
                {ctx.label("row", ctx.dayname(i), f'width: {L["day_col_width"]} ')}
                Label{{ width: {L["icon_col_width"]} text: "{emoji}" draw_text.text_style.font_size: 16 }}
                {ctx.label("row_dim", ctx.wx(f"daily.temperature_2m_min.{i}") + ' + "°"', f'width: {L["temp_col_width"]} align: Align{{x: 1.0}} padding: Inset{{right: {L["temp_inner_pad"]}}} ')}
                TempBar{{ width: Fill height: {L["bar_height"]} margin: Inset{{left: {L["bar_margin"]} right: {L["bar_margin"]}}}
                         draw_bg.tlo: {ctx.wxn(f"daily.temperature_2m_min.{i}")} draw_bg.thi: {ctx.wxn(f"daily.temperature_2m_max.{i}")}
                         draw_bg.wmin: {wmin} draw_bg.wmax: {wmax} }}
                {ctx.label("row", ctx.wx(f"daily.temperature_2m_max.{i}") + ' + "°"', f'width: {L["temp_col_width"]} align: Align{{x: 0.0}} padding: Inset{{left: {L["temp_inner_pad"]}}} ')}
            }}''')
    return (f'        RoundedView{{ width: Fill height: Fit flow: Down new_batch: true '
            f'draw_bg.color: {ctx.t["color"]["panel"]} draw_bg.border_radius: {L["panel_radius"]} '
            f'margin: Inset{{top: {L["section_gap"]}}} padding: Inset{{left: 16 right: 16 top: 10 bottom: 10}}\n'
            + "\n".join(rows) + "\n        }")

def air_quality_field(ctx, args):
    """The 16 uniforms are generated HERE. The plan never sees a uniform."""
    L = ctx.L
    unis = "\n".join(
        "                " + "  ".join(
            f'draw_bg.a{i}: sys.aqigrid({ctx.ll()}, 1.6, {i})' for i in range(r * 4, r * 4 + 4))
        for r in range(4))
    return f'''        {ctx.label("caption", json.dumps(ctx.S["air_quality"], ensure_ascii=False), f'margin: Inset{{top: {L["section_gap"]} bottom: 6}} ')}
        View{{ width: Fill height: {L["map_height"]} flow: Overlay
            Image{{ src: http_resource(sys.basemap({ctx.ll()})) fit: ImageFit.CropToFill width: Fill height: {L["map_height"]} }}
            AqiContour{{ width: Fill height: {L["map_height"]}
{unis}
            }}
        }}'''

def sun_moon(ctx, args):
    L = ctx.L
    return f'''        RoundedView{{ width: Fill height: Fit flow: Down new_batch: true draw_bg.color: {ctx.t["color"]["panel"]} draw_bg.border_radius: {L["panel_radius"]} margin: Inset{{top: {L["section_gap"]}}} padding: Inset{{left: 16 right: 16 top: 14 bottom: 14}} spacing: 10
            {ctx.label("caption", json.dumps(ctx.S["sun"], ensure_ascii=False))}
            SunArc{{ width: Fill height: 96 draw_bg.progress: sys.daylight({ctx.ll()}) }}
            View{{ width: Fill height: Fit flow: Right
                {ctx.label("row_dim", ctx.wx("daily.sunrise.0"))}
                Filler{{}}
                {ctx.label("row_dim", ctx.wx("daily.sunset.0"))}
            }}
            View{{ width: Fill height: Fit flow: Right align: Align{{y: 0.5}} spacing: 14 margin: Inset{{top: 8}}
                MoonPhase{{ width: 72 height: 72 draw_bg.phase: sys.moonnum("phase") }}
                View{{ width: Fill height: Fit flow: Down spacing: 4
                    {ctx.label("condition", f'sys.moonphase("{"name" if ctx.locale == "en" else "name_zh"}")')}
                    {ctx.label("tile_sub", f'sys.moonphase("illumination") + {json.dumps(ctx.S["illuminated"], ensure_ascii=False)}')}
                }}
            }}
        }}'''

TILES = {
    "aqi":      ("current.us_aqi",                 "",       "airquality"),
    "uv":       ("daily.uv_index_max.0",           "",       "weather"),
    "humidity": ("current.relative_humidity_2m",   ' + "%"', "weather"),
    "wind":     ("current.wind_speed_10m",         ' + " km/h"', "weather"),
    "pressure": ("current.surface_pressure",       ' + " hPa"', "weather"),
    "visibility": ("current.visibility",           ' + " m"',  "weather"),
}

def details(ctx, args):
    L, out = ctx.L, []
    tiles = args["tiles"]
    for r in range(0, len(tiles), 2):
        cells = []
        for key in tiles[r:r + 2]:
            path, unit, src = TILES[key]
            call = (f'sys.airquality({ctx.ll()}, "{path}")' if src == "airquality"
                    else ctx.wx(path)) + unit
            cells.append(f'''                RoundedView{{ width: Fill height: Fit flow: Down draw_bg.color: {ctx.t["color"]["tile"]} draw_bg.border_radius: {L["tile_radius"]} padding: Inset{{left: 14 top: 12 right: 14 bottom: 12}} spacing: 6
                    {ctx.label("tile_cap", json.dumps(ctx.S[key], ensure_ascii=False))}
                    {ctx.label("tile_val", call)}
                }}''')
        out.append(f'            View{{ width: Fill height: Fit flow: Right spacing: 10 margin: Inset{{top: 10}}\n'
                   + "\n".join(cells) + "\n            }")
    return (f'        View{{ width: Fill height: Fit flow: Down margin: Inset{{top: {L["section_gap"]}}}\n'
            + "\n".join(out) + "\n        }")

BLOCKS = {"CurrentConditions": current_conditions, "Forecast": forecast,
          "AirQualityField": air_quality_field, "SunMoon": sun_moon, "Details": details}

# -------------------------------------------------------------------- lower

def lower(plan, theme):
    ctx, L = Ctx(plan, theme), theme["layout"]
    body = "\n".join(BLOCKS[s["block"]](ctx, s.get("args", {})) for s in plan["sections"])
    p = L["page_padding"]
    # EXACTLY ONE top-level node, by construction. The dark base colour goes on
    # the root itself — a sibling SolidView would lay out beside it and take half
    # the screen's width.
    return f'''// name: weather-app
// GENERATED from a semantic plan — do not edit.
SolidView{{ width: Fill height: {L["root_height"]} flow: Overlay new_batch: true draw_bg.color: {theme["color"]["base"]}
    Image{{ src: http_resource(sys.photo({json.dumps(plan["photo"])})) fit: ImageFit.CropToFill width: Fill height: {L["photo_height"]} }}
    SolidView{{ width: Fill height: Fill draw_bg.color: {theme["color"]["scrim"]} }}
    View{{ width: Fill height: Fit flow: Down padding: Inset{{left: {p["left"]} top: {p["top"]} right: {p["right"]} bottom: {p["bottom"]}}}
{body}
    }}
}}
'''

if __name__ == "__main__":
    plan  = json.load(open(sys.argv[1]))
    reg   = json.load(open(sys.argv[2]))
    theme = json.load(open(sys.argv[3]))
    try:
        validate(plan, reg)
    except Reject as e:
        sys.exit(f"PLAN REJECTED: {e}")
    sys.stdout.write(lower(plan, theme))
