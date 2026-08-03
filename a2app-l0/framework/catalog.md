# Catalog: every role and capability a card may name

**Generated from `Splash/docs/ui-l0-constructors.toml` — do not edit.**
That file is what the checker enforces; a hand-kept copy would drift the
first time a role is added, and the card would be refused for using
something this document promised.

## Roles

| role | arguments |
|---|---|
| `AqiContour` | `lat: path`, `lon: path`, `span: number` |
| `Card` | `on_tap: event`, `value: any` |
| `Chip` | `text: text`, `on_tap: event`, `value: any`, `active: bool` |
| `Col` | `align: .start | .center | .end`, `gap: number` |
| `Field` | `text: path`, `placeholder: data`, `on_commit: event`, `width: width` |
| `Grid` | `cols: number` |
| `Map` | `mode: .plan | .drive | .flat`, `from: path`, `to: path`, `via: path`, `zoom: number` |
| `MoonPhase` | `phase: path`, `illum: path` |
| `Panel` | — |
| `Photo` | `src: path`, `pad: .page | .tight | .none` |
| `Row` | `align: .start | .center | .end | .baseline`, `gap: number`, `on_tap: event`, `value: any` |
| `Rule` | — |
| `StockPlot` | `symbol: path`, `range: unit` |
| `SunArc` | `rise: path`, `set: path`, `now: path` |
| `Surface` | `pad: .page | .tight | .none` |
| `TempBar` | `lo: path`, `hi: path`, `min: path`, `max: path` |
| `TextBody` | `text: text`, `width: width` |
| `TextCaption` | `text: text`, `value: data`, `glyph: text`, `suffix: text`, `unit: unit`, `width: width` |
| `TextHero` | `text: text`, `value: data`, `unit: unit`, `format: format`, `on_tap: event` |
| `TextRow` | `text: text`, `width: width` |
| `TextStat` | `value: data`, `format: format`, `tint: path` |
| `TextTitle` | `text: text`, `width: width` |
| `TextValue` | `value: data`, `unit: unit`, `format: format`, `tint: path` |
| `Tile` | `label: text`, `value: data`, `unit: unit`, `format: format` |
| `WeatherIcon` | `cond: path`, `size: .hero | .row | .tile` |

## Capabilities

A `source` may name one of these and nothing else.

| capability | arguments |
|---|---|
| `sys.airquality` | `lat`, `lon` |
| `sys.daylight` | `lat`, `lon` |
| `sys.geocode` | `name` |
| `sys.gps` | — (no arguments) |
| `sys.locale` | — (no arguments) |
| `sys.moonphase` | `lat`, `lon` |
| `sys.movers` | `count`, `fields` |
| `sys.news` | `count`, `offset`, `fields` |
| `sys.news_item` | `id`, `fields` |
| `sys.photo` | `query` |
| `sys.places` | `lat`, `lon`, `category`, `count`, `fields` |
| `sys.quote` | `ticker`, `fields` |
| `sys.route` | `from`, `to`, `via`, `mode`, `fields` |
| `sys.search` | `query`, `count`, `fields` |
| `sys.series` | `ticker`, `range`, `points`, `fields`, `aggregate` |
| `sys.weather` | `lat`, `lon`, `days`, `fields`, `aggregate` |

## Shared token sets

| set | tokens |
|---|---|
| `format` | ? |
| `unit` |  |
| `width` | ? |
