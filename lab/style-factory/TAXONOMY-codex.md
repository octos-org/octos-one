    geometric_sans
    humanist_sans
    oldstyle_serif_sans
    didone_sans
    slab_sans
    mono_sans
    pixel_mono
  ratio: [1.20, 1.25, 1.333, 1.50, 1.618_display_only]
  hierarchy: [quiet, clear, poster]

geometry:
  vocabulary:
    rectilinear
    circular_geometric
    soft_round
    pill_control
    mixed_geometric
    chamfered
    organic

contrast:
  primary:
    value
    scale
    weight
    hue
    spatial_isolation
    edge
    direction
  secondary:
    none
    value
    scale
    weight
    hue
    spatial_isolation
    edge

figure_ground:
  open
  contained
  tonal_strata
  layered
  ambiguous

media:
  mode: [none, abstract_geometry, illustration, photography, texture]
  dominance: [supporting, hero, background]

ornament:
  none
  rules
  geometric_primitives
  organic_paths
  pattern
  rough_texture
  glow_scanline
```

Do not independently randomize every resolved field. Resolution order should be:

1. Select `composition_school`, `surface_model`, and optional `digital_dialect`.
2. Apply their hard constraints.
3. Sample only the remaining legal primitive values.
4. Validate recognition and accessibility.
5. Compile one resolved recipe into THEME, COMPOSITION, ORNAMENT, and MEDIA outputs for both HTML and DSL.

For generic sampling, set media probabilities around:

- None: 40%
- Abstract geometry: 30%
- Illustration: 15%
- Texture: 10%
- Photography: 5%

Photography can be raised when the product content actually calls for it. It should not be the generator’s shortcut to visual richness.

## Canonical school constraints

| Profile | Must resolve to | Hard exclusions |
|---|---|---|
| **Swiss** | Asymmetric grid; regular rhythm; neo-grotesk; ratio 1.20–1.333; open/hairline surface; restrained palette | Neon glow, hard-offset shadow, glass, relief, skeuomorphism, diagonal composition, syncopated rhythm, decorative gradient, pill-everything |
| **Bauhaus** | Geometric composition; flat/outline surface; geometric sans; primary triad or achromatic-plus-primary | Organic blobs, glass, neumorphic relief, soft photographic background, decorative serif |
| **Constructivist** | Diagonal/layered composition; dense space; bold condensed/geometric type; red-black-cream | Centered calm layout, pastel-low-contrast palette, soft rounded cards, glass, relief |
| **De Stijl** | Modular orthogonal grid; square geometry; flat surface; primary triad | Every diagonal, curve, gradient, shadow, glow, blob, or pill |
| **Art Deco** | Axial or stepped composition; Didone/geometric display; tracked caps; hairline/double-frame language | Memphis scatter, organic blobs, hard-offset shadow, Material components, pixel icons |
| **Art Nouveau** | Organic geometry; open/layered composition; analogous or muted jewel palette; decorative serif | Strict square modular grid, chamfered HUD shapes, pixel type, hard-offset shadow |
| **Japanese MA** | Open-field composition; void ratio ≥0.55; muted achromatic/mono/analogous palette; ratio ≤1.25 | Dense dashboard, vivid triad, heavy outline, glow, collage texture, many equal cards |
| **Memphis** | Split/triadic vivid palette; mixed geometry; syncopated rhythm; flat-outline or hard-offset surface | Achromatic restraint, no ornament, strict Swiss regularity, glass, relief, low contrast |
| **Editorial** | Asymmetric grid or narrative stack; serif/sans pair; ratio ≥1.333; open/hairline figure-ground | Pill-everything, hard-offset shadow, glass, relief, pixel type, equal-card dashboard |
| **Punk/Zine** | Layered or diagonal composition; acid spot color; high scale contrast; rough texture | Soft Material elevation, regular component grid, MA-like silence, polished pastel softness |
| **Organic** | Open/layered flow; humanist or old-style type; analogous palette; organic masks | Chamfered HUD geometry, strict modular grid, heavy black outline, pixel/glitch treatment |

## Surface-model exclusions

These are primary material models and should be mutually exclusive.

- `open_flat`: no card shadow, bevel, backdrop blur, or relief.
- `tonal_material`: no hard-offset shadow, chrome bevel, inset relief, or decorative glow.
- `hard_offset`: requires a substantial stroke and zero-blur shadow; excludes glass, soft elevation, gradients, and low-contrast boundaries.
- `glass`: requires a nonuniform backdrop, overlap, alpha, and blur; exclude uniform-background glass and heavy black outlines.
- `relief`: requires page and surfaces to share the same hue family, paired shadows, and rounded geometry; exclude high-contrast card fills and square corners.
- `skeuomorphic`: requires a specific material metaphor. A generic gradient plus shadow is not sufficient.

## Digital-dialect exclusions

- **Y2K** requires chrome, glass, bevel, or lens-like depth. Exclude quiet MA, strict Swiss, rough zine texture, and no-ornament flatness.
- **Vaporwave** requires a gradient plus at least one temporal motif—glow, grid, scanline, or nostalgic image. Exclude achromatic flatness and natural muted palettes.
- **Cyberpunk** requires dark key, dense or layered composition, angular geometry, and emissive contrast. Exclude pastel softness, large-radius neumorphism, and ceremonial whitespace.
- **Retro-pixel** requires square geometry, a limited palette, pixel/mono type, and integer-grid icons. Exclude blur, smooth shadow, glass, organic masks, and continuous glossy gradients.

If a deliberately contradictory combination is desired—Swiss plus neon glow, for example—label it `fusion`, not `swiss`. Canonical mode should reject it.

# 5. Migration from the current labels

| Current label | Correct decomposition |
|---|---|
| `cinematic-photo` | `media=photography`, `dominance=background/hero`, cinematic value/chroma treatment |
| `material-light` | `surface=tonal_material`, `key=light` |
| `dense-feed` | `composition=single_column/dense_dashboard`, `void_ratio=0.20–0.30`, ratio 1.20 |
| `editorial-serif` | `school=editorial`, serif/sans pair, ratio 1.333–1.50 |
| `dark-terminal` | `key=near_black`, mono pair, dense regular grid; optionally `dialect=retro_pixel` |
| `glass-vibrant` | `surface=glass`, `chroma=vivid`, layered figure-ground |
| `newspaper` | `school=editorial`, `key=paper`, achromatic, dense columns, hairlines |
| `pastel-soft` | high-lightness muted palette, analogous/monochrome harmony, soft-round geometry |
| `brutalist` | Explicitly choose raw web brutalism or `surface=hard_offset` Neubrutalism |
| `neon-night` | `key=near_black`, vivid complementary/split palette, selective glow |

# Engineering priority

The highest-value change is not another theme token. It is a `COMPOSITION` layer capable of:

- Grid columns and spans
- Explicit void regions
- Overlay/layering
- Crop and bleed
- Axis and alignment
- Modular, alternating, and syncopated rhythm
- Optional rotation and clipping

Keep those directives outside the semantic card vocabulary.

After that:

1. Add `font_pair`, `leading_ratio`, and `tracking_em`.
2. Replace border/elevation booleans with real stroke and shadow parameters.
3. Add alpha, backdrop blur, masks, and an ornament plane.
4. Give every school profile a `required_capabilities` list. Refuse to generate an image-model mockup when either HTML or DSL cannot compile those capabilities.

If only one new token can be added now, add `font_pair`. If one architectural change can be made, build `COMPOSITION`. Those two changes create more genuine stylistic range than dozens of new vibe labels.

[exited with code 0]
