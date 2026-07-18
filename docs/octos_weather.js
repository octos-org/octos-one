/* octos.weather — a domain kit built ON octos.core; a second DATA-BOUND
 * component alongside octos.stock. octos.forecast("Tokyo") GEOCODES the place
 * AND fetches the 7-day forecast AND renders the card. Two keyless, CORS-open
 * open-meteo calls (no proxy needed). One call from the card; brick does the
 * rest — the web twin of a native sys.weather() helper. */
(function () {
  var O = (window.octos = window.octos || {});
  if (O.weather && O.weather._v) return;
  var W = (O.weather = O.weather || {}); W._v = 1;
  var C = O.core;

  /* ---------- weather CSS (built on core theme) ---------- */
  var CSS = `
.ow-card{position:relative;margin:10px 12px;padding:16px 16px 13px;background:var(--o-card);border:1px solid var(--o-line);border-radius:16px;overflow:hidden}
.ow-loc{font-size:16px;font-weight:700;letter-spacing:-.2px}
.ow-sub{font-size:12px;color:var(--o-mut);margin-top:1px;min-height:15px}
.ow-now{display:flex;align-items:center;gap:12px;margin-top:8px}
.ow-temp{font-size:46px;font-weight:300;letter-spacing:-2px;line-height:1}
.ow-cond{flex:1;min-width:0}
.ow-cond .c{font-size:14px;font-weight:600}
.ow-cond .f{font-size:11.5px;color:var(--o-mut);margin-top:2px;line-height:1.4}
.ow-ic{width:50px;height:50px;color:#eaeaea;flex-shrink:0}
.ow-ic svg{width:50px;height:50px}
.ow-days{display:flex;gap:6px;margin-top:14px;overflow-x:auto;scrollbar-width:none}
.ow-days::-webkit-scrollbar{display:none}
.ow-day{flex:0 0 auto;width:52px;text-align:center;padding:8px 0 7px;border-radius:11px;background:#ffffff08}
.ow-day .dow{font-size:11px;color:var(--o-mut);font-weight:600}
.ow-day svg{width:26px;height:26px;margin:6px auto 5px;color:#dcdcdc;display:block}
.ow-day .hl{font-size:11px;color:var(--o-mut)}
.ow-day .hl b{font-weight:700;color:var(--o-fg)}
.ow-load{animation:ow-pulse 1.15s ease-in-out infinite}
@keyframes ow-pulse{50%{opacity:.72}}
`;
  var st = document.createElement("style"); st.id = "octos-weather-css"; st.textContent = CSS;
  (document.head || document.documentElement).appendChild(st);

  /* ---------- weather icons (extend the shared map; full-SVG values) ---------- */
  Object.assign(O.icons, {
    w_clear: '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="5" fill="currentColor"/><path d="M12 2v2.2M12 19.8V22M2 12h2.2M19.8 12H22M4.9 4.9l1.6 1.6M17.5 17.5l1.6 1.6M19.1 4.9l-1.6 1.6M6.5 17.5l-1.6 1.6" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"/></svg>',
    w_pcloud: '<svg viewBox="0 0 24 24"><circle cx="16.5" cy="7" r="3" fill="currentColor" opacity=".8"/><path d="M7 20a4.2 4.2 0 0 1-.4-8.38A5 5 0 0 1 16 12.6 3.75 3.75 0 0 1 16 20z" fill="currentColor"/></svg>',
    w_cloud: '<svg viewBox="0 0 24 24"><path d="M7 19a4.5 4.5 0 0 1-.4-8.98A5.5 5.5 0 0 1 17 11 4 4 0 0 1 17 19z" fill="currentColor"/></svg>',
    w_fog: '<svg viewBox="0 0 24 24"><path d="M7 14.5a4.2 4.2 0 0 1-.4-8.38A5 5 0 0 1 16.5 7 3.75 3.75 0 0 1 16.5 14.5z" fill="currentColor" opacity=".92"/><path d="M4 18h16M6.5 21h11" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"/></svg>',
    w_rain: '<svg viewBox="0 0 24 24"><path d="M7 14.5a4.2 4.2 0 0 1-.4-8.38A5 5 0 0 1 16.5 7 3.75 3.75 0 0 1 16.5 14.5z" fill="currentColor"/><path d="M8 17.5l-1 3M12 17.5l-1 3M16 17.5l-1 3" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"/></svg>',
    w_snow: '<svg viewBox="0 0 24 24"><path d="M7 13.5a4.2 4.2 0 0 1-.4-8.38A5 5 0 0 1 16.5 6 3.75 3.75 0 0 1 16.5 13.5z" fill="currentColor"/><g fill="currentColor"><circle cx="8" cy="18" r="1.1"/><circle cx="12" cy="20" r="1.1"/><circle cx="16" cy="18" r="1.1"/></g></svg>',
    w_thunder: '<svg viewBox="0 0 24 24"><path d="M7 13.5a4.2 4.2 0 0 1-.4-8.38A5 5 0 0 1 16.5 6 3.75 3.75 0 0 1 16.5 13.5z" fill="currentColor"/><path d="M12.5 13l-3.2 4.3H12l-1 3.7 3.9-5.2H12l1.6-2.8z" fill="currentColor"/></svg>'
  });

  /* ---------- WMO weather code → [label, icon] ---------- */
  function wcode(c) {
    if (c === 0) return ["Clear sky", "w_clear"];
    if (c === 1) return ["Mainly clear", "w_pcloud"];
    if (c === 2) return ["Partly cloudy", "w_pcloud"];
    if (c === 3) return ["Overcast", "w_cloud"];
    if (c === 45 || c === 48) return ["Fog", "w_fog"];
    if (c >= 51 && c <= 57) return ["Drizzle", "w_rain"];
    if (c >= 61 && c <= 67) return ["Rain", "w_rain"];
    if (c >= 71 && c <= 77) return ["Snow", "w_snow"];
    if (c >= 80 && c <= 82) return ["Rain showers", "w_rain"];
    if (c === 85 || c === 86) return ["Snow showers", "w_snow"];
    if (c === 95) return ["Thunderstorm", "w_thunder"];
    if (c >= 96) return ["Thunderstorm, hail", "w_thunder"];
    return ["—", "w_cloud"];
  }
  W.wcode = wcode;

  /* ---------- data: open-meteo (CORS-open, keyless — direct fetch) ---------- */
  W.geocode = function (place) {
    return C.http.getJSON("https://geocoding-api.open-meteo.com/v1/search?count=1&language=en&name=" + encodeURIComponent(place))
      .then(function (j) { if (!j.results || !j.results[0]) throw new Error("no place"); var r = j.results[0];
        return { lat: r.latitude, lon: r.longitude, name: r.name, country: r.country || r.admin1 || "" }; });
  };
  W.data = function (lat, lon, opts) {
    var f = opts && opts.unit === "f";
    var u = "https://api.open-meteo.com/v1/forecast?latitude=" + lat + "&longitude=" + lon
      + "&current=temperature_2m,weather_code,relative_humidity_2m,wind_speed_10m,apparent_temperature"
      + "&daily=weather_code,temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=7"
      + (f ? "&temperature_unit=fahrenheit&wind_speed_unit=mph" : "");
    return C.http.getJSON(u).then(function (j) {
      return { now: { temp: j.current.temperature_2m, code: j.current.weather_code, feels: j.current.apparent_temperature,
                      wind: j.current.wind_speed_10m, hum: j.current.relative_humidity_2m },
        time: j.daily.time, code: j.daily.weather_code, max: j.daily.temperature_2m_max, min: j.daily.temperature_2m_min };
    });
  };

  /* ---------- the DATA-BOUND component: one call geocodes + fetches + renders ---------- */
  var uid = 0;
  W.forecast = function (place, opts) {
    opts = opts || {}; place = String(place || "").trim();
    var id = "ow-" + (++uid);
    setTimeout(function () { W._fill(id, place, opts); }, 0);
    var tap = opts.onTap || ("octos.weather.open('" + place.replace(/'/g, "") + "')");
    return '<div class="ow-card ow-load" id="' + id + '" onclick="' + tap + '">'
      + '<div class="ow-loc">' + C.esc(place || "Weather") + '</div><div class="ow-sub">Loading…</div>'
      + '<div class="ow-now"><div class="ow-temp">—°</div>'
      + '<div class="ow-cond"><div class="c">—</div><div class="f"></div></div><div class="ow-ic"></div></div>'
      + '<div class="ow-days"></div></div>';
  };
  W._fill = function (id, place, opts) {
    var geo = (opts.lat != null && opts.lon != null)
      ? Promise.resolve({ lat: opts.lat, lon: opts.lon, name: place, country: "" })
      : W.geocode(place);
    geo.then(function (g) { return W.data(g.lat, g.lon, opts).then(function (d) { return { g: g, d: d }; }); })
      .then(function (r) {
        var el = document.getElementById(id); if (!el) return;
        var g = r.g, d = r.d, unit = (opts.unit === "f") ? " mph" : " km/h";
        var wc = wcode(d.now.code);
        var days = d.time.map(function (t, i) {
          var dow = i === 0 ? "Today" : new Date(t + "T12:00:00").toLocaleDateString("en-US", { weekday: "short" });
          var wi = wcode(d.code[i]);
          return '<div class="ow-day"><div class="dow">' + dow + '</div>' + C.ic(wi[1])
            + '<div class="hl"><b>' + Math.round(d.max[i]) + '°</b> ' + Math.round(d.min[i]) + '°</div></div>';
        }).join("");
        el.classList.remove("ow-load");
        el.innerHTML =
          '<div class="ow-loc">' + C.esc(g.name || place) + '</div>'
          + '<div class="ow-sub">' + C.esc(g.country || "") + '</div>'
          + '<div class="ow-now"><div class="ow-temp">' + Math.round(d.now.temp) + '°</div>'
          + '<div class="ow-cond"><div class="c">' + wc[0] + '</div>'
          + '<div class="f">Feels ' + Math.round(d.now.feels) + '° · Humidity ' + d.now.hum + '% · Wind ' + Math.round(d.now.wind) + unit + '</div></div>'
          + '<div class="ow-ic">' + C.ic(wc[1]) + '</div></div>'
          + '<div class="ow-days">' + days + '</div>';
      }).catch(function () {
        var el = document.getElementById(id); if (!el) return;
        el.classList.remove("ow-load");
        var sub = el.querySelector(".ow-sub"); if (sub) sub.textContent = "Weather unavailable";
      });
  };
  W.open = function (p) { if (O.toast) O.toast(p + " · forecast", "OK"); };

  /* expose the component at root for one-line composition */
  O.forecast = W.forecast;
})();
