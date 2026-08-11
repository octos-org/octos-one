# convert — requirements

A unit converter: an amount, a pair of units, and the number between them.

Use it for "km to miles", "how many miles is 42 km", "kg to lbs", "20°C in
fahrenheit", "多少英里".

**Currency is NOT this card yet.** "usd to eur", "汇率", "convert 100 usd" need a
live rate, and there is no rate capability in the catalog. Writing one in is a
fact (§4) and the card is refused for it — correctly, because a rate typed into a
card is wrong within the hour. Answer a currency request with nothing rather than
with a number you invented.

`exemplar.card` meets every requirement below.

---

## What you fill in

Five states, and they are the entire brief:

```
state amount { shape: number, initial: 42 }         # what the request named
state factor { shape: number, initial: 0.621371 }   # from the table below
state offset { shape: number, initial: 0 }          # 32 for °C→°F, else 0
state dir    { shape: enum[fwd, rev], initial: .fwd }
```

plus the two `copy` labels naming the units (`copy pair_ab { en: "km → mi" }`
and its reverse), and the two bare unit names.

**`amount` comes from the request.** "how many miles is 42 km" → 42. Unspecified
→ 1, never 0: a converter that opens showing nothing has not answered.

**`dir` is which way the request asked.** "km to miles" is `.fwd` with the pair
written `km → mi`. Do not reorder the units to make `dir` `.fwd` — the pair
labels and the direction have to agree, and the Swap chip flips both.

---

## The sanctioned factors — the ONLY conversions you may write

| pair | factor | offset |
|---|---|---|
| km → mi | 0.621371 | 0 |
| m → ft | 3.28084 | 0 |
| cm → in | 0.393701 | 0 |
| kg → lb | 2.20462 | 0 |
| g → oz | 0.035274 | 0 |
| L → gal | 0.264172 | 0 |
| km/h → mph | 0.621371 | 0 |
| °C → °F | 1.8 | 32 |

A pair not in this table is a FAILED generation. Ask for one you have rather than
deriving one you half-remember.

**Why `offset` exists.** Temperature is affine, not proportional: °F is °C × 1.8
**+ 32**. Without an offset it would need its own card, and two cards that
convert are two cards that can disagree.

**The reverse direction is not a second formula.** It is the forward one solved
for the other side — `(amount − offset) ÷ factor` — so the card cannot drift into
two conversions that contradict each other. Both are already written in the
exemplar; you supply the numbers, not the algebra.

---

## What this card does NOT have

**No keypad.** The parent card had nineteen keys and an accumulator, and digit
entry is `amt = amt * 10 + d` — arithmetic in a TRANSITION. L0's transitions are
`set`, `toggle`, `cycle` and `clear`, deliberately. The amount comes from the
request and three preset chips adjust it.

**No typed amount.** `Field` binds a TEXT path; there is no numeric field role.
Do not bind `Field` to `amount` — the checker refuses it, and coercing a typed
string into a number-shaped state is not something the card can express.

---

## The card must also carry

- `# level: L1` — this app computes one value from numbers it already declared,
  which is exactly what L1 adds. Do not reach past it: no second expression form,
  no arithmetic anywhere but the two result values;
- the pair label and the Swap chip on one row, as complementary `when dir ==`
  guards so exactly one renders;
- the amount and its unit above the result, so the hero number is not a bare
  figure;
- three preset chips (1 / 10 / 100), each `active:` on its own value;
- `format: .ratio` on BOTH result values. An expression is computed in the
  backend, so nothing rounds it on the way out — 42 km came out as `26.097582`
  miles, which is correct and unreadable. One decimal is the answer a converter
  gives; the card asks for it and the runtime does the rounding.
