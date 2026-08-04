# map-pane — the reusable ZOOMABLE MAP pattern (declarative, true-centered)

A TAP-ZOOMABLE map any app can drop in: a 2×2 `sys.maptile` mosaic rendered
at DOUBLE size (744) inside the 372 pane and shifted by a computed negative
margin so the anchor sits EXACTLY at the pane's center — the 📍 pin is a
fixed dead-center overlay that never moves between zoom levels. Zoom is a
SINGLE state slot substituted into every sys call.

## The zoom state slot

Bind the zoom EVERYWHERE as `{{state.zoom|<DEFAULT>}}` — the `|<DEFAULT>`
(e.g. `|8`) renders while nothing is tapped, so the pane works on first
paint. The supported ladder is zoom **5–14**:

| z | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 |
|---|---|---|---|---|---|----|----|----|----|----|
| ~frame | 2400km | 1200km | 600km | 300km | 150km | 75km | 38km | 19km | 9km | 5km |

## The zoom controls: − / default / + (exactly THREE buttons)

The overlay bar holds the live level indicator on the left and THREE
translucent buttons on the right — a standard bounded stepper. `inc`/`dec`
step the level by 1 and clamp to [min, max]; `default` seeds the very first
step (must equal the slot default) and the middle button restores it:

```
Button{ text: "−" draw_text.text_style.font_size: 14 draw_bg.color: #000000aa padding: Inset{left: 12 top: 6 right: 12 bottom: 6} on_click: || agent.notify("dec", {key: "zoom", step: "1", min: "5", max: "14", default: "8"}) }
Button{ text: "8" draw_text.text_style.font_size: 14 draw_bg.color: #000000aa padding: Inset{left: 12 top: 6 right: 12 bottom: 6} on_click: || agent.notify("set", {key: "zoom", value: "8"}) }
Button{ text: "+" draw_text.text_style.font_size: 14 draw_bg.color: #000000aa padding: Inset{left: 12 top: 6 right: 12 bottom: 6} on_click: || agent.notify("inc", {key: "zoom", step: "1", min: "5", max: "14", default: "8"}) }
```

An app may narrow `min`/`max` to its domain (a country-scale app 5–9, a
city app 10–14) and pick its own default, but it is ALWAYS these three
buttons — never a row of preset-level chips. The bar sits INSIDE the pane's
Overlay, pushed to the bottom with a `Filler`:

```
View{ width: Fill height: Fill flow: Down
    Filler{}
    View{ width: Fill height: Fit flow: Right spacing: 6 padding: Inset{left: 10 right: 10 bottom: 10} align: Align{y: 0.5}
        Label{ text: "z" + "{{state.zoom|8}}" draw_text.color: #ff9f0a draw_text.text_style.font_size: 12 }
        Filler{}
        /* the three buttons here: − default + */
    }
}
```

## The pane (copy this shape; ONE mosaic, zoom = the slot)

`LATN`/`LONN` are the anchor's NUMERIC coords (e.g. `sys.quakesnum(0,"lat")`
or `sys.geocodenum("<place>","lat")` — write the full call each time). The
mosaic child is 744×744 (each tile an explicit 372×372) and its `margin`
subtracts the anchor's mosaic offset (`sys.mappin(..., 744)`) from the pane
half-size (186) — a NEGATIVE margin that slides the mosaic so the anchor
lands at the pane center; the pane clips the overflow:

```
RoundedView{ width: Fill height: 372 draw_bg.border_radius: 20 flow: Overlay new_batch: true
    View{ width: 744 height: 744 flow: Down
        margin: Inset{ left: 186 - sys.mappin(LATN, LONN, {{state.zoom|8}}, "x", 744) top: 186 - sys.mappin(LATN, LONN, {{state.zoom|8}}, "y", 744) }
        View{ width: 744 height: 372 flow: Right
            Image{ src: http_resource(sys.maptile(LATN, LONN, {{state.zoom|8}}, "tl")) fit: ImageFit.CropToFill width: 372 height: 372 }
            Image{ src: http_resource(sys.maptile(LATN, LONN, {{state.zoom|8}}, "tr")) fit: ImageFit.CropToFill width: 372 height: 372 }
        }
        View{ width: 744 height: 372 flow: Right
            Image{ src: http_resource(sys.maptile(LATN, LONN, {{state.zoom|8}}, "bl")) fit: ImageFit.CropToFill width: 372 height: 372 }
            Image{ src: http_resource(sys.maptile(LATN, LONN, {{state.zoom|8}}, "br")) fit: ImageFit.CropToFill width: 372 height: 372 }
        }
    }
    View{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5}
        Label{ text: "📍" draw_text.text_style.font_size: 22 margin: Inset{bottom: 22} }
    }
}
```

## Hard rules

- Every `sys.maptile`/`sys.mappin` call in the pane uses the SAME
  `{{state.zoom|<DEFAULT>}}` slot and the SAME default, and the stepper's
  `default` payload equals that slot default.
- Every Image is the EXPLICIT 372×372 and the mosaic wrapper the explicit
  744 sizes — never `Fill` (Fill inside an Overlay pane resolves against
  the card root and shows only a strip).
- The pin is the FIXED center overlay above (align x/y 0.5, bottom-margin
  22 so the pin TIP marks the anchor) — never positioned with per-zoom
  offsets; it must not move when the zoom changes.
- The ONLY overlay children are the mosaic, the pin View, and the bottom
  control bar — no caption pills, no scrims. Captions go OUTSIDE the pane.
- The pane MUST be followed by the tile attribution line OUTSIDE the pane:
  `Label{ text: "© OpenStreetMap contributors © CARTO" }` (11px, the app's
  tertiary text color). The keyless CARTO basemap tiles require this credit
  in the UI — never drop it, never shrink it into the Overlay.
- Exactly THREE control buttons (−, the default level, +) with
  `step`/`min`/`max`/`default` payloads as above; min/max within 5–14.
