# Interaction & state patterns (client-side, no LLM round-trip)

Generic building blocks for interactive cards. Every card owns its state keys;
writing a key re-renders the card body with the new value — the LLM is NOT
called again. Combine these patterns freely to assemble multi-view apps.

## Reading state in a full-script body

A stateful card is a full-script body: the first lines (right after the
`// name:` line) are `let` declarations reading state, then view code:

```
let tab = "{{state.tab}}"
if tab == "" || tab == "a" { View{ /* view A */ } } else { View{ /* view B */ } }
```

- An unset key renders as `""` — treat `""` as your default in every condition.
- Branch WHOLE views with `if/else`; both branches must be complete views.

## Writing state: any Button

```
Button{ text: "Go" on_click: || agent.notify("set", {key: "tab", value: "b"}) }
```

An invisible tap target is a transparent Button: `draw_bg.color: #00000000
draw_bg.color_hover: #00000000 draw_bg.color_focus: #00000000
draw_bg.color_down: #00000000 draw_bg.border_size: 0.0`.

## Tappable list row (overlay pattern)

A fixed-height row with a full-size transparent Button overlaid to catch taps:

```
View{ width: Fill height: 64 flow: Overlay
    View{ width: Fill height: Fill flow: Right align: Align{y: 0.5}
        /* row content: labels, values … */
    }
    Button{ width: Fill height: Fill text: "" draw_bg.color: #00000000 draw_bg.color_hover: #00000000 draw_bg.color_focus: #00000000 draw_bg.color_down: #00000000 draw_bg.border_size: 0.0 on_click: || agent.notify("set", {key: "selected", value: "ROW-VALUE"}) }
}
```

## Selectable chip row (active underline driven by state)

ALL chips stay visible in EVERY state. A chip is a small Overlay: the label +
underline (the visual) with a full-size TRANSPARENT `Button` on top (the tap).
Per the design system, the ACTIVE chip is bright text + a 2dp accent underline;
the inactive chip is dim text + a transparent 2dp spacer (row height stable).
⚠️ Never use a translucent filled pill for the active state — tinted fills in
nested overlays do not render reliably. Only the styling branches on the state
value; remember `""` means the default chip:

```
View{ width: Fill height: Fit flow: Right align: Align{y: 0.5}
    View{ width: Fit height: Fit flow: Overlay
        if tab == "" || tab == "a" {
            View{ width: Fit height: Fit flow: Down padding: Inset{left: 10 top: 5 right: 10 bottom: 3}
                Label{ text: "A" draw_text.color: #30d158 draw_text.text_style.font_size: 13 }
                SolidView{ width: Fill height: 2 new_batch: true draw_bg.color: #30d158 margin: Inset{top: 3} } }
        } else {
            View{ width: Fit height: Fit flow: Down padding: Inset{left: 10 top: 5 right: 10 bottom: 3}
                Label{ text: "A" draw_text.color: #ffffff66 draw_text.text_style.font_size: 13 }
                SolidView{ width: Fill height: 2 new_batch: true draw_bg.color: #00000000 margin: Inset{top: 3} } }
        }
        Button{ width: Fill height: Fill text: "" draw_bg.color: #00000000 draw_bg.color_hover: #00000000 draw_bg.color_focus: #00000000 draw_bg.color_down: #00000000 draw_bg.border_size: 0.0 on_click: || agent.notify("set", {key: "tab", value: "a"}) }
    }
    Filler{}
    /* repeat the same chip Overlay per value ("b", "c", …) */
}
```

**The key `tab` and values `"a"/"b"/"c"` above are PLACEHOLDERS.** Always
substitute the key name and values YOUR app's spec declares (e.g. a chart-range
chip row writes key `range` with values `"1d" "1w" "1m" "6m" "1y"`); a chip
that writes the wrong key re-renders the card with nothing changed.

Never render a chip row as bare Labels — static chips that don't write state
are a FAILURE in any app that declares a switchable control.

## Splash-LOCAL state + named widgets (no re-render, no LLM, no agent.notify)

For nav INSIDE one card (list↔detail) the fastest pattern is local mutation:
a full-script body opens with a plain state object and `fn` helpers that
mutate NAMED widgets in place — the card never re-renders:

```
let app = { detail: false selected: 0 }

fn show_item(i) {
    app.detail = true
    app.selected = i
    ui.header_btn.set_text("< Back")
    ui.lead_title.set_text(sys.news(i, "title"))
}
fn show_list() { app.detail = false ui.header_btn.set_text("SECTIONS") }
fn header_click() { if app.detail { show_list() } }

SolidView{ width: Fill height: 780 flow: Overlay new_batch: true
    /* … header_btn := ButtonFlatter{ on_click: || header_click() } … */
}
```

⚠️ **Closures are EXPRESSION form ONLY**: `on_click: || one_fn_call(args)`.
The block form `on_click: ||{ ... }` does NOT work — `{ ... }` after `||`
parses as an OBJECT LITERAL (the `let app = { … }` syntax), so the handler
silently never fires. Put ALL branching inside `fn` bodies (like
`header_click` above) and call ONE fn from the closure.

- Name a widget with `id := Widget{…}`; mutate it via `ui.<id>.set_text(…)`.
- The state object line (`let app = { … }`) must be the FIRST executable line
  after `// name:`, and no comment may precede it.
- Use this pattern when tap→change is instant and self-contained; use
  `{{state.key}}` + `agent.notify("set", …)` (above) when a branch of the
  card must re-render with different STRUCTURE.

## Style templates (`let X = Widget{…}`) — reuse without repetition

Define a style ONCE, instantiate many times:

```
let Row = RoundedView{ width: Fill height: 136 flow: Overlay new_batch: true draw_bg.color: #ffffff0d draw_bg.border_radius: 8. }
let Tap = ButtonFlatter{ width: 72 height: Fill text: "" grab_key_focus: false draw_bg.color_focus: #00000000 padding: 0 }

Row{ /* children */ Tap{ on_click: || show_item(1) } }
```

⚠️ Template rules in this embedded renderer: an instantiation supplies its
live `View`/`Label` children DIRECTLY (nested property overrides are
ignored), and do NOT define extra base `View`/`Label` prototypes — they leak
defaults into unrelated widgets. Define only the templates you instantiate.

## Scrolling lists + tap targets (drag-gesture safety)

Put ONLY the scrolling rows in a `ScrollYView{ width: Fill height: Fill
flow: Down }`; keep mastheads/leads/back buttons OUTSIDE it so they stay
fixed. NEVER cover a whole row inside a ScrollYView with a full-size button —
it captures drag gestures and kills scrolling. Give each row a transparent
trailing tap target instead (the 72dp `Tap` template above, aligned with a
`>` chevron); the row body stays a swipe surface.
