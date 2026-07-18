# Design system — global styles for ALL apps (enforced)

This is the stylesheet: every app card MUST draw from these tokens. An app's
`app.md` defines structure and content; THIS file defines the look. Never
invent a new color or font size when a token fits.

## Color tokens

- `bg/base` `#0a0e14` · `bg/gradient-end` `#0e1826` (GradientYView, top→bottom)
- `surface/frosted` `#ffffff0d` — stat cards, sheets (RoundedView + `new_batch: true`)
- `hairline` `#ffffff1a` (separators, gridlines; `#ffffff10` for the dimmer mid-gridline)
- `text/primary` `#ffffff` · `text/secondary` `#ffffff8c` · `text/dim` `#ffffff66` · `text/faint` `#ffffff59`
- `accent/positive` `#30d158` · `accent/negative` `#ff453a`

## Type scale (`draw_text.text_style.font_size`)

- `display` 42 — hero value (a price, a temperature)
- `title` 32–34 — screen title / symbol
- `heading` 22 — row primary text, stat values
- `body` 15–16 — change lines, buttons
- `caption` 13 — names, chips, eyebrows
- `micro` 10–12 — axis labels, stat labels

Pick FROM the scale; no in-between sizes.

## Spacing & layout

- Screen: root `SolidView{ width: Fill height: 858 flow: Overlay new_batch: true }`
  over `bg/base`, `GradientYView` underlay, content `View` padded
  `Inset{left: 22 top: 42 right: 22 bottom: 118}`.
- Vertical rhythm: 12–20 between sections, 4–8 within a group. Radii: pills 12,
  cards/sheets 16.
- Rows: `flow: Right` + `align: Align{y: 0.5}`; space-between via `Filler{}`;
  list rows `height: Fit` with `padding: Inset{top: 10 bottom: 10}` — content
  must NEVER clip.
- Layers: `flow: Overlay`; ANY tinted or solid surface layered over the
  gradient needs `new_batch: true`.
- Separators: `SolidView{ width: Fill height: 1 }` hairline.

## Emphasis rules

- Directional values (price change, deltas): `accent/positive` when up,
  `accent/negative` when down — NEVER a green negative or red positive.
- Active selection (chips, tabs): bright text (`accent/positive` or
  `text/primary`) plus a 2dp underline bar (`SolidView{ width: Fill height: 2
  new_batch: true }` in the accent color) under the label; inactive =
  `text/dim` with a transparent 2dp spacer so the row height never shifts.
  **Do NOT use translucent filled pills for selection** — tinted fills inside
  nested overlays do not render reliably; the underline is the standard.
