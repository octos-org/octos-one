# chart — requirements

A comparison card: several countries' readings of **one World Bank indicator**,
drawn on one axis with a legend, and each country's own numbers beneath it.

Use it for any "compare these countries on this measure" request: "china gdp
growth vs india", "show me india vs vietnam gdp per capita", "life expectancy
in japan and korea over 60 years", "中国和印度的 GDP 增速".

`exemplar.card` is a working card that meets every requirement below. Read it
first — it is shorter than this document.

---

## What you fill in

Three states, and they are the entire brief:

```
state countries { shape: text, initial: "CHN,IND" }              # ISO3, in reading order
state metric    { shape: text, initial: "NY.GDP.MKTP.KD.ZG" }    # a World Bank code
state span      { shape: number, initial: 30 }                   # years back
```

**`countries` is ISO3, comma-separated, and the ORDER IS THE READING ORDER.**
"China vs India" is `"CHN,IND"`, not `"IND,CHN"` — the chart assigns its legend
colours in this order and the rows beneath it follow, so the first country the
user named must be first here. Mapping a name to ISO3 is world knowledge and
yours: China → CHN, India → IND, United States → USA, Vietnam → VNM, Japan →
JPN, Korea → KOR, Germany → DEU, Brazil → BRA, Nigeria → NGA, Indonesia → IDN.
At most **five**; a sixth is dropped rather than drawn.

**`metric` is a World Bank indicator code** — also yours to know. The ones worth
reaching for:

| Request says | code |
|---|---|
| GDP growth, "how fast is it growing" | `NY.GDP.MKTP.KD.ZG` |
| GDP, the size of the economy | `NY.GDP.MKTP.CD` |
| GDP per capita, "how rich" | `NY.GDP.PCAP.CD` |
| inflation | `FP.CPI.TOTL.ZG` |
| life expectancy | `SP.DYN.LE00.IN` |
| population | `SP.POP.TOTL` |
| unemployment | `SL.UEM.TOTL.ZS` |
| CO₂ per capita | `EN.GHG.CO2.PC.CE.AR5` |
| electricity access (%) | `EG.ELC.ACCS.ZS` |
| internet users (%) | `IT.NET.USER.ZS` |

If the request names a measure not in this table, pick the World Bank code you
know for it — the card renders whatever the API answers, and says so plainly
when the API has nothing.

**`span` is years back from the present**, a plain number: "past 30 years" → 30,
"since 1990" → 36, unspecified → 30.

---

## What you must NOT do

**Never write a number from the series.** Not a growth rate, not a GDP figure,
not "China averaged 8.5%". The card names WHO and WHAT and the runtime fetches
it; a figure written into the card is a fact (§4) and it is wrong the moment the
World Bank revises the series — which they do, backwards, every year.

**Never invent an indicator code.** A code the World Bank does not publish
renders "no data for that indicator", which is honest. A code that exists but
measures something else renders a confident chart of the wrong thing.

---

## The shape

```
source read  sys.indicator(countries: state.countries,
                           indicator: state.metric,
                           years: state.span,
                           fields: [name, latest, first, change, min, max, year, title])
```

One row per country, in `countries` order. `read.title` is the indicator's own
name as the API spells it — use it as the card's title rather than restating the
request. Every row carries `name` (as the API spells the country), `latest`,
`first`, `change`, `min`, `max`, `year`.

```
IndicatorPlot(countries: countries, indicator: metric, years: span)
```

The chart. It fetches the same request the source does — one network call serves
the whole card — scales every series to one viewport so the comparison is
honest, and draws its own legend. It takes no colours: which line is which
colour is the theme's to decide.

The card must also carry:

- the §5.9 lifecycle guards (`read.$state == .pending` / `.failed`) with copy
  that says which one it is;
- a **span** row (10y / 30y / 60y chips writing `span`);
- a **metric** row of chips for the two or three codes closest to the request,
  each `active:` on its own value so the theme lights the current one;
- one **row per country** under the chart, in source order, carrying that
  country's `name`, `latest` and `change`.
