/* octos.finance — a domain kit built ON octos.core, and the reference for a
 * DATA-BOUND component: octos.stock("AAPL") both FETCHES the live quote and
 * RENDERS the tile. The card writes ONE call; the brick does its own homework.
 * This is the web fusion of a widget + a sys.* data helper. */
(function () {
  var O = (window.octos = window.octos || {});
  if (O.finance && O.finance._v) return;
  var F = (O.finance = O.finance || {}); F._v = 1;
  var C = O.core;

  /* ---------- finance CSS (built on core theme) ---------- */
  var CSS = `
.of-stock{position:relative;margin:10px 12px;padding:14px 15px;background:var(--o-card);border:1px solid var(--o-line);border-radius:14px;overflow:hidden}
.of-stock .of-hd{display:flex;align-items:baseline;gap:8px}
.of-tk{font-size:15px;font-weight:700;letter-spacing:.4px}
.of-nm{font-size:12px;color:var(--o-mut);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;flex:1;min-width:0}
.of-px{font-size:26px;font-weight:700;margin-top:7px;letter-spacing:-.5px}
.of-ch{font-size:13px;font-weight:600;margin-top:2px;color:var(--o-mut)}
.of-up .of-ch{color:var(--o-up)}
.of-down .of-ch{color:var(--o-down)}
.of-spark{position:absolute;right:0;bottom:0;width:46%;height:48px;opacity:.9;pointer-events:none}
.of-load .of-px,.of-load .of-nm{opacity:.35}
.of-load{animation:of-pulse 1.15s ease-in-out infinite}
@keyframes of-pulse{50%{opacity:.72}}
`;
  var st = document.createElement("style"); st.id = "octos-finance-css"; st.textContent = CSS;
  (document.head || document.documentElement).appendChild(st);

  /* ---------- formatting ---------- */
  F._n = function (n) { return (Math.round(n*100)/100).toLocaleString(undefined, {minimumFractionDigits:2, maximumFractionDigits:2}); };
  F._cur = function (c) { return ({USD:"$",EUR:"€",GBP:"£",JPY:"¥",CNY:"¥",HKD:"HK$",INR:"₹",KRW:"₩",AUD:"A$",CAD:"C$"})[c] || (c?c+" ":""); };
  F._spark = function (arr, up) {
    if (!arr || arr.length < 2) return "";
    var mn = Math.min.apply(null, arr), mx = Math.max.apply(null, arr), rng = (mx-mn)||1, n = arr.length;
    var pts = arr.map(function (v, i) { var x = i/(n-1)*100, y = 30 - (v-mn)/rng*28; return x.toFixed(1)+","+y.toFixed(1); });
    var col = up ? "var(--o-up)" : "var(--o-down)";
    return '<polyline fill="none" stroke="'+col+'" stroke-width="1.7" stroke-linejoin="round" points="'+pts.join(" ")+'"/>';
  };

  /* ---------- data: Yahoo v8 chart via the core CORS proxy ---------- */
  F.quote = function (ticker) {
    var url = "https://query1.finance.yahoo.com/v8/finance/chart/" + encodeURIComponent(ticker) + "?range=1d&interval=5m&includePrePost=false";
    return C.http.getJSONx(url).then(function (j) {
      var r = j.chart.result[0], m = r.meta;
      var price = m.regularMarketPrice;
      var prev = (m.chartPreviousClose != null) ? m.chartPreviousClose : m.previousClose;
      var closes = ((r.indicators && r.indicators.quote && r.indicators.quote[0].close) || []).filter(function (x) { return x != null; });
      if (price == null) throw new Error("no price");
      if (closes.length) price = closes[closes.length-1];
      return { ticker: ticker.toUpperCase(), name: m.shortName || m.longName || "", cur: F._cur(m.currency),
        price: price, change: price - prev, pct: prev ? (price-prev)/prev*100 : 0, spark: closes };
    });
  };

  /* ---------- the DATA-BOUND component: one call fetches + renders ---------- */
  var uid = 0;
  F.stock = function (ticker, opts) {
    opts = opts || {}; ticker = String(ticker || "").toUpperCase();
    var id = "of-stk-" + (++uid);
    setTimeout(function () { F._fill(id, ticker); }, 0);          // fetch after this paint
    var tap = opts.onTap || ("octos.finance.open('" + ticker + "')");
    return '<div class="of-stock of-load" id="' + id + '" onclick="' + tap + '">'
      + '<div class="of-hd"><span class="of-tk">' + C.esc(ticker) + '</span><span class="of-nm">—</span></div>'
      + '<div class="of-px">—</div><div class="of-ch">Loading…</div>'
      + '<svg class="of-spark" viewBox="0 0 100 32" preserveAspectRatio="none"></svg></div>';
  };
  F._fill = function (id, ticker) {
    F.quote(ticker).then(function (d) {
      var el = document.getElementById(id); if (!el) return;
      var up = d.change >= 0, sg = up ? "+" : "";
      el.classList.remove("of-load");
      el.classList.toggle("of-up", up); el.classList.toggle("of-down", !up);
      el.querySelector(".of-nm").textContent = d.name;
      el.querySelector(".of-px").textContent = d.cur + F._n(d.price);
      el.querySelector(".of-ch").textContent = sg + F._n(d.change) + " (" + sg + d.pct.toFixed(2) + "%)  ·  today";
      el.querySelector(".of-spark").innerHTML = F._spark(d.spark, up);
    }).catch(function () {
      var el = document.getElementById(id); if (!el) return;
      el.classList.remove("of-load");
      el.querySelector(".of-ch").textContent = "Quote unavailable";
    });
  };
  F.open = function (t) { if (O.toast) O.toast(t + " · live quote", "OK"); };

  /* expose the component at root for one-line composition */
  O.stock = F.stock;
})();
