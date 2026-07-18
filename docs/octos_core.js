/* octos.core — the domain-agnostic primitives every octos web card composes from.
 * The web counterpart of Splash `glass.*` core: theme + reset, namespaced state,
 * avatar, a shared icon map, toast / bottom-sheet / dim, and http (with a CORS
 * proxy for keyless third-party APIs). Domain kits (media, finance, …) are built
 * ON this and never redefine it. The common primitives are also aliased onto the
 * root `octos.*` for ergonomic composition. */
(function () {
  var O = (window.octos = window.octos || {});
  if (O.core && O.core._v) return;
  var C = (O.core = O.core || {});
  C._v = 1;

  /* ---------- theme tokens + reset + general primitives ---------- */
  var CSS = `
:root{--o-bg:#0f0f0f;--o-fg:#f1f1f1;--o-mut:#9aa0a6;--o-card:#181818;--o-line:#262626;--o-chip:#272727;--o-up:#28c76f;--o-down:#ea5455;--o-accent:#3ea6ff}
*{margin:0;padding:0;box-sizing:border-box;-webkit-tap-highlight-color:transparent}
html,body{background:var(--o-bg);color:var(--o-fg);font-family:Roboto,Arial,sans-serif}
.o-hidden{display:none!important}
.o-empty{color:#717171;font-size:12.5px;padding:2px 12px 8px}
/* avatar (shared) */
.o-ava{position:relative;width:36px;height:36px;border-radius:50%;display:flex;align-items:center;justify-content:center;font-weight:700;font-size:15px;color:#fff;flex-shrink:0;box-shadow:inset 0 0 0 1px rgba(255,255,255,.08);overflow:hidden}
.o-ava img{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;border-radius:50%}
/* general card primitives (.oc-*) — for any non-media card */
.oc-card{margin:10px 12px;padding:14px 15px;background:var(--o-card);border:1px solid var(--o-line);border-radius:14px}
.oc-hd{display:flex;align-items:center;gap:10px}
.oc-title{font-size:15px;font-weight:600}
.oc-sub{font-size:12.5px;color:var(--o-mut);margin-top:2px}
.oc-row{display:flex;align-items:center;gap:12px;padding:12px 14px;border-bottom:1px solid var(--o-line)}
.oc-row:last-child{border-bottom:0}
.oc-btn{height:34px;padding:0 15px;border-radius:17px;border:0;background:var(--o-chip);color:var(--o-fg);font-size:13px;font-weight:600;display:inline-flex;align-items:center;gap:6px}
.oc-btn.pri{background:var(--o-fg);color:var(--o-bg)}
.oc-btn svg{width:17px;height:17px;fill:currentColor}
/* dim + bottom sheet + toast (shared infra) */
.o-dim{position:fixed;inset:0;background:rgba(0,0,0,.65);z-index:60;opacity:0;pointer-events:none;transition:opacity .2s}
.o-dim.on{opacity:1;pointer-events:auto}
.o-sheet{position:fixed;left:0;right:0;bottom:0;z-index:61;background:#212121;border-radius:16px 16px 0 0;transform:translateY(120%);opacity:0;visibility:hidden;transition:transform .22s ease-out,opacity .2s,visibility .22s;padding:4px 0 52px;box-shadow:0 -6px 26px rgba(0,0,0,.55)}
.o-sheet.on{transform:translateY(0);opacity:1;visibility:visible}
.o-sheet .handle{width:36px;height:4px;border-radius:2px;background:#4d4d4d;margin:6px auto 10px}
.o-sheet button{display:flex;align-items:center;gap:16px;width:100%;border:0;background:transparent;color:#f1f1f1;font-size:14px;padding:13px 20px;text-align:left}
.o-sheet svg{width:20px;height:20px;fill:#f1f1f1}
.o-toast{position:fixed;left:12px;right:12px;bottom:64px;background:#212121;color:#f1f1f1;font-size:13.5px;padding:13px 16px;border-radius:8px;opacity:0;transition:opacity .25s,transform .25s;transform:translateY(8px);pointer-events:none;z-index:99;box-shadow:0 4px 16px rgba(0,0,0,.5);display:flex;align-items:center}
.o-toast .ta{margin-left:auto;color:var(--o-accent);font-weight:600;padding-left:14px}
`;
  var st = document.createElement("style"); st.id = "octos-core-css"; st.textContent = CSS;
  (document.head || document.documentElement).appendChild(st);

  /* ---------- text helpers ---------- */
  C.esc = function (s) { return C.strip(String(s)).replace(/&/g, "&amp;").replace(/</g, "&lt;"); };
  C.strip = function (s) { return String(s).replace(/[←-⯿️⭐]|[\uD83C-\uD83E][\uDC00-\uDFFF]/g, "").replace(/\s{2,}/g, " ").trim(); };

  /* ---------- avatar (generic; optional image url) ---------- */
  var A1 = ["#7b5cff","#ff5c8a","#2fb3ff","#28c76f","#ff9f43","#ea5455","#00cfe8","#a05cff"];
  var A2 = ["#3b2d80","#8a2d4c","#155a80","#146b3c","#8a5a1e","#7a2020","#00707e","#54258a"];
  C.avaStyle = function (n) { var h=0; for (var i=0;i<n.length;i++) h=(h*31+n.charCodeAt(i))|0; var j=Math.abs(h)%A1.length; return "background:linear-gradient(135deg,"+A1[j]+","+A2[j]+")"; };
  C.avatar = function (name, size, fs, img) { name=name||"?"; var im=img?'<img src="'+img+'" onerror="this.remove()" loading="lazy">':""; return '<div class="o-ava" style="'+C.avaStyle(name)+';width:'+size+'px;height:'+size+'px;font-size:'+fs+'px">'+name[0].toUpperCase()+im+"</div>"; };

  /* ---------- state (namespaced localStorage; one card at a time) ---------- */
  var NS = "app.";
  C.ns = function (p) { NS = p; };
  C.get = function (k, d) { try { var v = localStorage.getItem(NS+k); return v?JSON.parse(v):d; } catch(e){ return d; } };
  C.set = function (k, v) { try { localStorage.setItem(NS+k, JSON.stringify(v)); } catch(e){} };

  /* ---------- icon map (shared; domain kits Object.assign more) ---------- */
  O.icons = O.icons || {};
  Object.assign(O.icons, {
    play:"M8 5v14l11-7z", pause:"M6 19h4V5H6v14zm8-14v14h4V5h-4z",
    search:"M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z",
    close:"M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z",
    back:"M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z",
    home:"M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z",
    person:"M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z",
    chev:"M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6z",
    check:"M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z",
    plus:"M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6z",
    gear:"M19.14 12.94c.04-.31.06-.63.06-.94s-.02-.63-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.31-.09.64-.09.94s.02.63.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z",
    refresh:"M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z",
    star:"M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z"
  });
  C.ic = function (n) { var v = O.icons[n]; if (!v) return ""; return v.charAt(0)==="<" ? v : '<svg viewBox="0 0 24 24"><path d="'+v+'"/></svg>'; };

  /* ---------- toast / bottom-sheet / dim (shared infra) ---------- */
  var dim, sheet, toastEl;
  function infra() {
    if (dim) return;
    dim = document.createElement("div"); dim.className = "o-dim"; dim.onclick = C.closeSheet;
    sheet = document.createElement("div"); sheet.className = "o-sheet";
    toastEl = document.createElement("div"); toastEl.className = "o-toast";
    document.body.appendChild(dim); document.body.appendChild(sheet); document.body.appendChild(toastEl);
  }
  C.toast = function (m, a) { infra(); toastEl.innerHTML = C.esc(m) + (a?'<span class="ta">'+a+"</span>":""); toastEl.style.opacity=1; toastEl.style.transform="translateY(0)"; clearTimeout(toastEl._h); toastEl._h=setTimeout(function(){ toastEl.style.opacity=0; toastEl.style.transform="translateY(8px)"; }, 1700); };
  C.sheet = function (title, items) { infra();
    sheet.innerHTML = '<div class="handle"></div>' + (title?'<div style="font-size:15px;font-weight:600;padding:6px 20px 10px">'+C.esc(title)+"</div>":"")
      + items.map(function (it,i){ return '<button data-i="'+i+'">'+C.ic(it.icon)+C.esc(it.label)+"</button>"; }).join("");
    Array.prototype.forEach.call(sheet.querySelectorAll("button"), function (b){ b.onclick=function(e){ e.stopPropagation(); var it=items[+b.dataset.i]; C.closeSheet(); if(it.on)it.on(); }; });
    dim.classList.add("on"); sheet.classList.add("on");
  };
  C.closeSheet = function () { if(dim){dim.classList.remove("on");} if(sheet){sheet.classList.remove("on");} };

  /* ---------- http (+ CORS proxy for keyless third-party APIs) ---------- */
  C.http = {
    /* wrap a URL so a keyless, CORS-less API becomes fetchable from the card origin */
    proxy: function (url) { return "https://api.allorigins.win/raw?url=" + encodeURIComponent(url); },
    getJSON: function (url, o) { return fetch(url, o).then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); }); },
    getText: function (url, o) { return fetch(url, o).then(function (r) { if (!r.ok) throw new Error(r.status); return r.text(); }); },
    /* JSON from a host without CORS — routed through the proxy */
    getJSONx: function (url, o) { return C.http.getJSON(C.http.proxy(url), o); }
  };

  /* ---------- expose common primitives at root for composition + back-compat ---------- */
  ["esc","strip","avaStyle","avatar","ns","get","set","ic","toast","sheet","closeSheet","http"].forEach(function (k) {
    if (O[k] === undefined) O[k] = C[k];
  });
})();
