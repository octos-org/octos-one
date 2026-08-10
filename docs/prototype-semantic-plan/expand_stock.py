#!/usr/bin/env python3
"""
Lower a STOCK plan to makepad Splash DSL — the second-domain test.

The question step 6 exists to answer: does the block idea generalise past weather,
or is it templates with extra ceremony?

Measured answer, from writing this:

  * ZERO blocks reuse. MoversList / QuoteHeader / PriceChart / RangeChips /
    StatGrid share nothing with CurrentConditions / Forecast / SunMoon. Expected —
    blocks ARE the domain.
  * The INFRASTRUCTURE reuses almost entirely: the same validator (imported, not
    copied), the same text roles, the same theme-token shape, the same
    "framework owns invariants" rules (root height, one top-level node, live data
    via helpers, no typed literals).
  * Three genuinely NEW schema concepts were forced, and weather never needed any
    of them: STATE, ACTIONS, and VIEWS.

That third point is the finding. Weather is a pure function of (place, locale) and
renders one screen. Stocks has a selected ticker and a chart range that the USER
changes, and two screens chosen by that state. So a static plan cannot express it:
the plan needs declared state, blocks need to emit declared actions, and views need
a predicate over state.

That is not a weather-shaped problem with a stocks label on it — it is the
identity/mutation problem arriving on schedule. A plan can DECLARE
`onTap: select`, but something has to keep the selected row's widget alive across
the re-render, route the tap back into the VM, and patch rather than rebuild. This
prototype lowers the declaration; it does not implement the session contract.
See step 9.

Usage: expand_stock.py <plan.json> <blocks.json> > card.splash
"""
import json, sys, os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from expand import validate, Reject          # SAME validator as weather + ArkUI

# ---------------------------------------------------------------- theme

TH = {
    "color": {
        "base": "#0b0d12", "panel": "#141821", "tile": "#ffffff14",
        "hair": "#ffffff1a", "up": "#32d74b", "down": "#ff453a",
        "text": "#ffffff", "dim": "#ffffff99", "faint": "#ffffff66",
        "chip": "#ffffff14", "chip_on": "#ffffff33",
    },
    "layout": {
        "root_height": "Fit", "page_padding": {"left": 20, "top": 54, "right": 20, "bottom": 24},
        "panel_radius": 18.0, "tile_radius": 14.0, "row_pad": 10, "section_gap": 14,
        "chart_height": 160, "card_height": 1500,
    },
    "strings": {
        "en": {"open": "OPEN", "high": "HIGH", "low": "LOW", "prev": "PREV CLOSE",
               "volume": "VOLUME", "mktcap": "MKT CAP", "back": "‹ Movers"},
        "zh": {"open": "开盘", "high": "最高", "low": "最低", "prev": "昨收",
               "volume": "成交量", "mktcap": "市值", "back": "‹ 涨幅榜"},
    },
}

def C(k):
    return TH["color"][k]

class Ctx:
    """Same shape as the weather Ctx — this is the reused infrastructure."""
    def __init__(self, plan):
        self.locale = plan["locale"]
        self.S = TH["strings"][self.locale]
        self.L = TH["layout"]

    def deref(self, v):
        """`$selected` / `$range` refer to declared STATE, not literals.

        The plan never contains a ticker the model made up: it either names one
        the user asked for or points at state. Same rule as PlaceRef — the model
        may say WHICH thing, never what the data is.
        """
        return v[1:] if isinstance(v, str) and v.startswith("$") else json.dumps(v)  # $x -> the state let

    def txt(self, role, expr, color="text", extra=""):
        return f'{role}{{ {extra}text: {expr} draw_text.color: {C(color)} }}'



# The real interaction primitive octos-one already provides: a full-size
# TRANSPARENT Button whose on_click writes a state key via agent.notify. No LLM
# round-trip — writing a key re-renders the card body. `MoversTap`/`RangeTap` were
# my invention and do not exist; a declared action lowers to this.
def tap(key, value):
    return ('Button{ width: Fill height: Fill text: "" '
            'draw_bg.color: #00000000 draw_bg.color_hover: #00000000 '
            'draw_bg.color_focus: #00000000 draw_bg.color_down: #00000000 '
            'draw_bg.border_size: 0.0 '
            f'on_click: || agent.notify("set", {{key: "{key}", value: {value}}}) }}')

# --------------------------------------------------------------- blocks

def movers_list(ctx, args):
    rows = []
    n = args.get("count", 10)
    for i in range(n):
        rows.append(f'''            View{{ width: Fill height: 56 flow: Overlay
              View{{ width: Fill height: Fill flow: Right align: Align{{y: 0.5}}
                {ctx.txt("TextRow", f'"{i + 1}"', "faint", "width: 28 ")}
                View{{ width: Fill height: Fit flow: Down
                    {ctx.txt("TextRow", f'sys.movers({i}, "symbol")')}
                    {ctx.txt("TextCaption", f'sys.movers({i}, "name")', "dim")}
                }}
                View{{ width: Fit height: Fit flow: Down align: Align{{x: 1.0}}
                    {ctx.txt("TextRow", f'"$" + sys.movers({i}, "price")')}
                    {ctx.txt("TextCaption", f'sys.movers({i}, "changepct")',
                             "up" if True else "down")}
                }}
              }}
                {tap("selected", f'sys.movers({i}, "symbol")') if args.get("onTap") else ''}
            }}
            SolidView{{ width: Fill height: 1 draw_bg.color: {C("hair")} }}''')
    return f'''        {ctx.txt("TextCaption", json.dumps(args.get("eyebrow", "")), "up")}
        {ctx.txt("TextHero", json.dumps(args.get("title", "")))}
        RoundedView{{ width: Fill height: Fit flow: Down new_batch: true
            draw_bg.color: {C("panel")} draw_bg.border_radius: {ctx.L["panel_radius"]}
            margin: Inset{{top: {ctx.L["section_gap"]}}} padding: Inset{{left: 14 right: 14 top: 6 bottom: 6}}
{chr(10).join(rows)}
        }}'''

def quote_header(ctx, args):
    t = ctx.deref(args["ticker"])
    return f'''        View{{ width: Fill height: Fit flow: Down
            {ctx.txt("TextCaption", json.dumps(ctx.S["back"]), "dim")}
            {ctx.txt("TextTitle", f'sys.stock({t}, "symbol")')}
            {ctx.txt("TextCaption", f'sys.stock({t}, "name") + " · " + sys.stock({t}, "currency")', "dim")}
            {ctx.txt("TextHero", f'"$" + sys.stock({t}, "price")', "text", 'margin: Inset{top: 2} ')}
            {ctx.txt("TextStat", f'sys.stockrange({t}, range, "change") + "  (" + sys.stockrange({t}, range, "changepct") + ")"', "up")}
        }}'''

def price_chart(ctx, args):
    # A straight pass-through: StockPlot already takes semantic args. This block
    # is the precedent AqiContour was refactored to match.
    return (f'        StockPlot{{ width: Fill height: {ctx.L["chart_height"]} '
            f'symbol: {ctx.deref(args["ticker"])} range: {ctx.deref(args["range"])} '
            f'margin: Inset{{top: {ctx.L["section_gap"]}}} }}')

def range_chips(ctx, args):
    chips = []
    for r in args["ranges"]:
        chips.append(f'''            RoundedView{{ width: Fit height: Fit draw_bg.border_radius: 12.0
                draw_bg.color: {C("chip")} padding: Inset{{left: 12 right: 12 top: 6 bottom: 6}}
                {ctx.txt("TextCaption", json.dumps(r), "dim")}
                {tap("range", json.dumps(r)) if args.get("onTap") else ''}
            }}''')
    return (f'        View{{ width: Fill height: Fit flow: Right spacing: 8 '
            f'margin: Inset{{top: 10}}\n' + chr(10).join(chips) + "\n        }")

STATS = {"open": "open", "high": "high", "low": "low",
         "prev": "prev", "volume": "volume", "mktcap": "mktcap"}

def stat_grid(ctx, args):
    t = ctx.deref(args["ticker"])
    cells = [f'''                RoundedView{{ width: Fill height: Fit flow: Down
                    draw_bg.color: {C("tile")} draw_bg.border_radius: {ctx.L["tile_radius"]}
                    padding: Inset{{left: 12 right: 12 top: 10 bottom: 10}} spacing: 4
                    {ctx.txt("TextCaption", json.dumps(ctx.S[k], ensure_ascii=False), "dim")}
                    {ctx.txt("TextValue", f'sys.stock({t}, "{STATS[k]}")')}
                }}''' for k in args["stats"]]
    rows = []
    for i in range(0, len(cells), 2):
        rows.append('            View{ width: Fill height: Fit flow: Right spacing: 10 margin: Inset{top: 10}\n'
                    + chr(10).join(cells[i:i + 2]) + "\n            }")
    return (f'        View{{ width: Fill height: Fit flow: Down margin: Inset{{top: {ctx.L["section_gap"]}}}\n'
            + chr(10).join(rows) + "\n        }")

BLOCKS = {"MoversList": movers_list, "QuoteHeader": quote_header,
          "PriceChart": price_chart, "RangeChips": range_chips, "StatGrid": stat_grid}

# ---------------------------------------------------------------- lower

def lower(plan):
    ctx = Ctx(plan)
    L = ctx.L
    p = L["page_padding"]
    views = []
    for v in plan["views"]:
        body = "\n".join(BLOCKS[s["block"]](ctx, s.get("args", {})) for s in v["sections"])
        views.append((v["when"], body))
    # State declared once, at the top, from the plan's own defaults.
    st = plan.get("state", {})
    decls = "\n".join(f'let {k} = "{{{{state.{k}}}}}"' for k in st)
    # Views become an if/else chain over state — the runtime owns the predicate
    # language, exactly as octos-pipeline's condition.rs owns DOT's gates.
    chain = ""
    for i, (when, body) in enumerate(views):
        kw = "if" if i == 0 else "} else if"
        chain += f'''        {kw} {when} {{
{body}
'''
    chain += "        }"
    return f'''// name: stock-app
// GENERATED from a semantic plan by expand_stock.py — do not edit.
{decls}

SolidView{{ width: Fill height: {L["root_height"]} flow: Overlay new_batch: true draw_bg.color: {C("base")}
    View{{ width: Fill height: Fit flow: Down padding: Inset{{left: {p["left"]} top: {p["top"]} right: {p["right"]} bottom: {p["bottom"]}}}
{chain}
    }}
}}
'''

if __name__ == "__main__":
    plan = json.load(open(sys.argv[1]))
    reg = json.load(open(sys.argv[2]))
    try:
        validate(plan, reg)
    except Reject as e:
        sys.exit(f"PLAN REJECTED: {e}")
    sys.stdout.write(lower(plan))
