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
| `Chip` | `text: text`, `on_tap: event`, `value: any`, `active: bool`, `tone: .normal \| .primary \| .danger` |
| `Col` | `align: .start \| .center \| .end`, `gap: number`, `width: width` |
| `Field` | `text: path`, `placeholder: data`, `on_commit: event`, `on_change: event`, `width: width` |
| `Grid` | `cols: number` |
| `Map` | `mode: .plan \| .drive \| .flat`, `from: path`, `to: path`, `via: path` |
| `MoonPhase` | `phase: path`, `illum: path` |
| `Panel` | `dock: .top \| .bottom \| .right` |
| `Photo` | `src: path`, `pad: .page \| .tight \| .none` |
| `Reveal` | — |
| `Row` | `width: width`, `align: .start \| .center \| .end \| .baseline`, `gap: number`, `on_tap: event`, `value: any` |
| `Rule` | — |
| `Satellite` | `lat: path`, `lon: path` |
| `StockPlot` | `symbol: path`, `range: unit` |
| `SunArc` | `rise: path`, `set: path`, `now: path` |
| `Surface` | `pad: .page \| .tight \| .none` |
| `TempBar` | `lo: path`, `hi: path`, `min: path`, `max: path` |
| `TextBody` | `text: text`, `width: width` |
| `TextCaption` | `text: text`, `value: data`, `glyph: text`, `suffix: text`, `unit: unit`, `width: width` |
| `TextHero` | `text: text`, `value: data`, `unit: unit`, `format: format`, `on_tap: event` |
| `TextRow` | `text: text`, `width: width` |
| `TextStat` | `value: data`, `format: format`, `tint: path` |
| `TextTitle` | `text: text`, `width: width` |
| `TextValue` | `value: data`, `unit: unit`, `format: format`, `tint: path` |
| `Tile` | `label: text`, `value: data`, `unit: unit`, `format: format` |
| `WeatherIcon` | `cond: path`, `size: .hero \| .row \| .tile` |

## Capabilities

A `source` may name one of these and nothing else.

**`answers` is the whole vocabulary.** A `fields:` list may name only
these, and a view may read only what its source asked for — both are
refused otherwise, because a field nobody can answer renders as missing
and that looks exactly like data still arriving.

| capability | arguments | answers |
|---|---|---|
| `sys.airquality` | `lat`, `lon` | `aqi`, `pm25`, `pm10`, `ozone` |
| `sys.cities` | `fields` | `name`, `lat`, `lon`, `temp`, `feels`, `hi`, `lo`, `cond`, `humidity`, `wind` |
| `sys.daylight` | `lat`, `lon` | `rise`, `set`, `now` |
| `sys.geocode` | `name` | `lat`, `lon`, `name`, `country`, `admin1`, `timezone`, `population` |
| `sys.gps` | — (no arguments) | `lat`, `lon`, `accuracy`, `ok` |
| `sys.locale` | — (no arguments) | `lang`, `temp_unit` |
| `sys.moonphase` | `lat`, `lon` | `phase`, `illumination`, `name` |
| `sys.movers` | `count`, `fields`, `symbols` | `ticker`, `name`, `last`, `change`, `pct`, `open`, `high`, `low`, `prev`, `volume`, `mktcap`, `pe`, `currency`, `exchange` |
| `sys.news` | `count`, `offset`, `fields` | `id`, `title`, `author`, `points`, `comments`, `url` |
| `sys.news_item` | `id`, `fields` | `id`, `title`, `author`, `points`, `comments`, `url` |
| `sys.photo` | `query` | — (not a record) |
| `sys.places` | `lat`, `lon`, `category`, `count`, `fields` | `id`, `name`, `distance`, `lat`, `lon`, `category` |
| `sys.prefs` | `fields` | `units`, `range`, `home`, `work`, `mode` |
| `sys.quote` | `ticker`, `fields` | `ticker`, `name`, `last`, `change`, `pct`, `open`, `high`, `low`, `prev`, `volume`, `mktcap`, `pe`, `currency`, `exchange` |
| `sys.reading` | `fields` | `id`, `title`, `author`, `points`, `comments`, `url` |
| `sys.route` | `from_lat`, `from_lon`, `to_lat`, `to_lon`, `via`, `mode`, `fields` | `duration`, `distance`, `steps` |
| `sys.search` | `query`, `count`, `fields` | `id`, `name`, `label`, `query`, `lat`, `lon`, `distance` |
| `sys.series` | `ticker`, `range`, `points`, `fields`, `aggregate` | `min`, `max`<br>`aggregate:` `min`, `max` |
| `sys.step` | `from_lat`, `from_lon`, `to_lat`, `to_lon`, `at_lat`, `at_lon`, `fields` | `instruction`, `remaining`, `progress`, `eta` |
| `sys.symbol_search` | `query`, `count`, `fields` | `ticker`, `name`, `exchange`, `kind` |
| `sys.watchlist` | `fields` | `ticker`, `name`, `last`, `change`, `pct`, `open`, `high`, `low`, `prev`, `volume`, `mktcap`, `pe`, `currency`, `exchange` |
| `sys.weather` | `lat`, `lon`, `days`, `fields`, `aggregate` | `temp`, `feels`, `hi`, `lo`, `cond`, `humidity`, `wind`, `pressure`, `uv`, `visibility`, `precip`, `dayname`, `days`<br>`aggregate:` `min_lo`, `max_hi` |

## Shared token sets

| set | tokens |
|---|---|
| `format` | .money \| .signed_money \| .signed_pct \| .compact \| .ratio \| .time \| .date |
| `mapview` | .flat \| .tilted |
| `unit` | .c \| .f \| .pct \| .speed \| .pressure \| .distance \| .money \| .duration |
| `width` | .fill \| .fit \| .day \| .rank \| .temp \| .label |
