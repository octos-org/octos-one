/* octos.media — the YouTube/video domain kit, built ON octos.core.
 * Owns the media-specific parts that must stay consistent: the player
 * (IFrame API + captions/translate + PiP), the bottom-sheet menus, and the
 * visual widgets (topbar, tiles, action-bar, channel-row, comments, chips).
 * A card provides data + layout + a rerender() hook; behavior is kit-owned. */
(function () {
  var O = (window.octos = window.octos || {});
  if (O.media && O.media._v) return;
  var M = (O.media = O.media || {}); M._v = 1;
  O._v = 1;                                   // legacy guard: old inlined kits short-circuit
  if (O.core) O.core.ns("yt.");               // media cards share the yt.* namespace

  /* ---------- media CSS (built on core theme) ---------- */
  var CSS = `
.o-live{color:#f11;font-weight:600}
/* top bar */
.o-topbar{display:flex;align-items:center;gap:4px;padding:10px 12px}
.o-logo{display:flex;align-items:center;gap:5px;font-size:18.5px;font-weight:700;letter-spacing:-.6px}
.o-logo svg{width:29px;height:20px}
.o-topbar .o-sp{flex:1}
.o-nav{width:38px;height:38px;border-radius:50%;border:0;background:transparent;color:#f1f1f1;display:flex;align-items:center;justify-content:center}
.o-nav svg{width:21px;height:21px;fill:#f1f1f1}
.o-you{width:30px;height:30px;border-radius:50%;border:0;padding:0;margin-left:6px;position:relative;overflow:hidden;color:#fff;font-weight:700;font-size:13px;display:flex;align-items:center;justify-content:center}
.o-you.on{box-shadow:0 0 0 2px #f1f1f1}
/* player */
.o-player{position:sticky;top:0;z-index:40;width:100%;height:56.25vw;background:#000 center/cover no-repeat;box-shadow:0 8px 0 #0f0f0f,0 10px 18px rgba(0,0,0,.7)}
.o-player:before{content:"";position:absolute;left:0;right:0;top:0;height:52px;background:linear-gradient(rgba(0,0,0,.5),transparent);pointer-events:none;z-index:3}
.o-player>div,.o-player iframe{width:100%;height:100%;border:0;display:block}
.o-min{position:absolute;left:6px;top:6px;z-index:8;width:36px;height:36px;border:0;border-radius:50%;background:transparent;display:flex;align-items:center;justify-content:center}
.o-min svg{width:22px;height:22px;fill:#fff}
.o-gest{position:absolute;inset:0;z-index:6;display:none}
body:not(.o-pip) .o-gest{display:block}
.o-fs{position:absolute;right:8px;bottom:8px;z-index:8;width:38px;height:38px;border:0;border-radius:50%;background:rgba(0,0,0,.45);display:none;align-items:center;justify-content:center}
body:not(.o-pip) .o-fs{display:flex}
.o-fs svg{width:22px;height:22px;fill:#fff}
/* pip */
body.o-pip .o-player:before,body.o-pip .o-min{display:none}
body.o-pip .o-player{position:fixed!important;top:auto!important;left:auto!important;right:10px;bottom:62px;width:150px;height:150px;min-height:0;z-index:57;border-radius:12px;overflow:hidden;background:#000;box-shadow:0 6px 22px rgba(0,0,0,.75)}
body:not(.o-pip) .o-pipov{display:none!important}
body.o-pip .o-pipov{display:block;position:absolute;inset:0;z-index:10;background:rgba(0,0,0,.14)}
.o-pipov .tog{position:absolute;left:50%;top:50%;transform:translate(-50%,-50%);width:46px;height:46px;border:0;border-radius:50%;background:rgba(0,0,0,.6);display:flex;align-items:center;justify-content:center}
.o-pipov .cls{position:absolute;top:5px;right:5px;width:30px;height:30px;border:0;border-radius:50%;background:rgba(0,0,0,.6);display:flex;align-items:center;justify-content:center}
.o-pipov .tog svg{width:24px;height:24px;fill:#fff}
.o-pipov .cls svg{width:18px;height:18px;fill:#fff}
/* meta + actions */
.o-title{font-size:16px;font-weight:600;line-height:1.35;margin:11px 12px 3px}
.o-meta{font-size:12.5px;color:#aaa;margin:0 12px 9px}
.o-actions{display:flex;gap:7px;overflow-x:auto;padding:2px 12px 13px;scrollbar-width:none}
.o-actions::-webkit-scrollbar{display:none}
.o-pill{flex-shrink:0;display:flex;align-items:center;gap:6px;height:32px;padding:0 13px;border-radius:16px;background:#272727;color:#f1f1f1;font-size:13px;font-weight:500;border:0}
.o-pill svg{width:17px;height:17px;fill:#f1f1f1}
.o-pill.act svg{fill:#fff}
.o-likewrap{flex-shrink:0;display:flex;height:32px;border-radius:16px;background:#272727;overflow:hidden}
.o-likewrap button{display:flex;align-items:center;gap:6px;border:0;background:transparent;color:#f1f1f1;font-size:13px;font-weight:500;padding:0 12px}
.o-likewrap .dv{width:1px;background:#3f3f3f;margin:6px 0}
.o-likewrap button.act svg{fill:#fff}
.o-likewrap svg{width:17px;height:17px;fill:#f1f1f1}
/* channel row */
.o-chrow{display:flex;align-items:center;gap:11px;padding:11px 12px;border-top:1px solid #222;border-bottom:1px solid #222}
.o-chname{font-size:14px;font-weight:600}
.o-chsub{font-size:11.5px;color:#aaa;margin-top:1px}
.o-chrow .o-sp{flex:1}
.o-sub{height:34px;padding:0 15px;border-radius:17px;border:0;background:#f1f1f1;color:#0f0f0f;font-size:13px;font-weight:600;display:flex;align-items:center;gap:6px}
.o-sub.on{background:#272727;color:#f1f1f1}
.o-sub svg{width:17px;height:17px;fill:#f1f1f1}
/* comments */
.o-cbox{margin:11px 12px;padding:11px 12px;background:#272727;border-radius:12px}
.o-chead{display:flex;align-items:baseline;gap:8px;font-size:13.5px;font-weight:600}
.o-ccount{color:#aaa;font-size:12px;font-weight:400}
.o-cmt{display:flex;gap:9px;margin-top:10px}
.o-cmt .o-ava{width:24px;height:24px;font-size:10px}
.o-cmt .b{font-size:12.5px;line-height:1.45}
.o-cmt .w{font-size:11px;color:#aaa;margin-bottom:1px}
.o-cghost{display:flex;gap:9px;margin-top:10px;align-items:center}
.o-cghost .f{flex:1;background:#121212;border-radius:16px;height:32px;display:flex;align-items:center;padding:0 13px;color:#717171;font-size:12.5px}
.o-crow{display:flex;gap:9px;margin-top:10px;align-items:center}
.o-cin{flex:1;background:#121212;border:1px solid #3f3f3f;border-radius:16px;height:34px;color:#f1f1f1;font-size:13px;padding:0 13px;outline:none}
.o-cpost{border:0;background:#3ea6ff;color:#0f0f0f;font-weight:600;font-size:12.5px;height:34px;padding:0 13px;border-radius:17px}
/* tiles + feed */
.o-sec{font-size:15px;font-weight:600;margin:14px 12px 8px;display:flex;align-items:baseline}
.o-sec .va{margin-left:auto;font-size:12px;color:#3ea6ff;font-weight:500}
.o-tile{display:flex;gap:10px;padding:5px 12px;position:relative}
.o-tile .th{position:relative;width:152px;flex-shrink:0}
.o-tile img{width:152px;height:85px;object-fit:cover;border-radius:10px;background:#272727;display:block}
.o-badge{position:absolute;right:5px;bottom:5px;background:rgba(0,0,0,.78);color:#fff;font-size:10px;font-weight:600;padding:1px 5px;border-radius:4px;letter-spacing:.3px}
.o-badge.lv{background:#f11}
.o-tile .tx{min-width:0;flex:1;padding-right:22px}
.o-tile .t{font-size:13px;font-weight:500;line-height:1.35;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
.o-tile .c{font-size:11.5px;color:#aaa;margin-top:3px}
.o-kebab{position:absolute;right:2px;top:2px;width:32px;height:32px;border:0;background:transparent;display:flex;align-items:center;justify-content:center}
.o-kebab svg{width:16px;height:16px;fill:#f1f1f1}
.o-chips{display:flex;gap:8px;overflow-x:auto;padding:2px 12px 12px;scrollbar-width:none}
.o-chips::-webkit-scrollbar{display:none}
.o-chip{flex-shrink:0;height:30px;padding:0 13px;border-radius:8px;background:#272727;color:#f1f1f1;font-size:13px;font-weight:500;border:0}
.o-chip.on{background:#f1f1f1;color:#0f0f0f}
.o-fcard{margin-bottom:16px;position:relative}
.o-fcard .th{position:relative;margin:0 12px}
.o-fcard img{width:100%;aspect-ratio:16/9;object-fit:cover;border-radius:12px;background:#272727;display:block}
.o-fcard .row{display:flex;gap:11px;padding:9px 12px 0}
.o-fcard .tx{flex:1;min-width:0;padding-right:20px}
.o-fcard .t{font-size:14px;font-weight:500;line-height:1.35;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
.o-fcard .c{font-size:12px;color:#aaa;margin-top:3px}
.o-fcard .o-kebab{position:static}
`;
  var st = document.createElement("style"); st.id = "octos-media-css"; st.textContent = CSS;
  (document.head || document.documentElement).appendChild(st);

  /* ---------- media icons (extend the shared map) ---------- */
  Object.assign(O.icons, {
    fs:"M7 14H5v5h5v-2H7v-3zm-2-4h2V7h3V5H5v5zm12 7h-3v2h5v-5h-2v3zM14 5v2h3v3h2V5h-5z",
    bell:"M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z",
    like_o:"M13.11 5.72l-.57 2.89c-.12.59.04 1.2.42 1.66.38.46.94.73 1.54.73H20v1.08L17.43 18H9.34c-.18 0-.34-.16-.34-.34V9.82l4.11-4.1M14 2 7.59 8.41C7.21 8.79 7 9.3 7 9.83v7.83C7 18.95 8.05 20 9.34 20h8.1c.71 0 1.36-.37 1.72-.97l2.67-6.15c.11-.25.17-.52.17-.8V11c0-1.1-.9-2-2-2h-5.5l.92-4.65c.05-.22.02-.46-.08-.66-.23-.45-.52-.86-.88-1.22L14 2zM4 9H2v11h2c.55 0 1-.45 1-1v-9c0-.55-.45-1-1-1z",
    like_f:"M1 21h4V9H1v12zm22-11c0-1.1-.9-2-2-2h-6.31l.95-4.57.03-.32c0-.41-.17-.79-.44-1.06L14.17 1 7.59 7.59C7.22 7.95 7 8.45 7 9v10c0 1.1.9 2 2 2h9c.83 0 1.54-.5 1.84-1.22l3.02-7.05c.09-.23.14-.47.14-.73v-2z",
    dis_o:"M10.89 18.28l.57-2.89c.12-.59-.04-1.2-.42-1.66-.38-.46-.94-.73-1.54-.73H4v-1.08L6.57 6h8.09c.18 0 .34.16.34.34v7.84l-4.11 4.1M10 22l6.41-6.41c.38-.38.59-.89.59-1.42V6.34C17 5.05 15.95 4 14.66 4h-8.1c-.71 0-1.36.37-1.72.97l-2.67 6.15c-.11.25-.17.52-.17.8V13c0 1.1.9 2 2 2h5.5l-.92 4.65c-.05.22-.02.46.08.66.23.45.52.86.88 1.22L10 22zm10-7h2V4h-2c-.55 0-1 .45-1 1v9c0 .55.45 1 1 1z",
    dis_f:"M15 3H6c-.83 0-1.54.5-1.84 1.22l-3.02 7.05c-.09.23-.14.47-.14.73v2c0 1.1.9 2 2 2h6.31l-.95 4.57-.03.32c0 .41.17.79.44 1.06L9.83 23l6.59-6.59c.36-.36.58-.86.58-1.41V5c0-1.1-.9-2-2-2zm4 0v12h4V3h-4z",
    share:"M18 16.08c-.76 0-1.44.3-1.96.77L8.91 12.7c.05-.23.09-.46.09-.7s-.04-.47-.09-.7l7.05-4.11c.54.5 1.25.81 2.04.81 1.66 0 3-1.34 3-3s-1.34-3-3-3-3 1.34-3 3c0 .24.04.47.09.7L8.04 9.81C7.5 9.31 6.79 9 6 9c-1.66 0-3 1.34-3 3s1.34 3 3 3c.79 0 1.5-.31 2.04-.81l7.12 4.16c-.05.21-.08.43-.08.65 0 1.61 1.31 2.92 2.92 2.92 1.61 0 2.92-1.31 2.92-2.92s-1.31-2.92-2.92-2.92z",
    save_o:"M17 3H7c-1.1 0-2 .9-2 2v16l7-3 7 3V5c0-1.1-.9-2-2-2zm0 15l-5-2.18L7 18V5h10v13z",
    save_f:"M17 3H7c-1.1 0-2 .9-2 2v16l7-3 7 3V5c0-1.1-.9-2-2-2z",
    remix:"M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z",
    report:"M14.4 6L14 4H5v17h2v-7h5.6l.4 2h7V6z",
    cc:"M19 4H5c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm-8 7H9.5v-.5h-2v3h2V13H11v1c0 .55-.45 1-1 1H7c-.55 0-1-.45-1-1v-4c0-.55.45-1 1-1h3c.55 0 1 .45 1 1v1zm7 0h-1.5v-.5h-2v3h2V13H18v1c0 .55-.45 1-1 1h-3c-.55 0-1-.45-1-1v-4c0-.55.45-1 1-1h3c.55 0 1 .45 1 1v1z",
    translate:"M12.87 15.07l-2.54-2.51.03-.03c1.74-1.94 2.98-4.17 3.71-6.53H17V4h-7V2H8v2H1v1.99h11.17C11.5 7.92 10.44 9.75 9 11.35 8.07 10.32 7.3 9.19 6.69 8h-2c.73 1.63 1.73 3.17 2.98 4.56l-5.09 5.02L4 19l5-5 3.11 3.11.76-2.04zM18.5 10h-2L12 22h2l1.12-3h4.75L21 22h2l-4.5-12zm-2.62 5l1.62-4.33L19.12 15h-3.24z",
    kebab:"M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z",
    bellsub:"M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z",
    eyeoff:"M12 7c2.76 0 5 2.24 5 5 0 .65-.13 1.26-.36 1.83l2.92 2.92c1.51-1.26 2.7-2.89 3.43-4.75-1.73-4.39-6-7.5-11-7.5-1.4 0-2.74.25-3.98.7l2.16 2.16C10.74 7.13 11.35 7 12 7zM2 4.27l2.28 2.28.46.46C3.08 8.3 1.78 10.02 1 12c1.73 4.39 6 7.5 11 7.5 1.55 0 3.03-.3 4.38-.84l.42.42L19.73 22 21 20.73 3.27 3 2 4.27zM7.53 9.8l1.55 1.55c-.05.21-.08.43-.08.65 0 1.66 1.34 3 3 3 .22 0 .44-.03.65-.08l1.55 1.55c-.67.33-1.41.53-2.2.53-2.76 0-5-2.24-5-5 0-.79.2-1.53.53-2.2zm4.31-.78l3.15 3.15.02-.16c0-1.66-1.34-3-3-3l-.17.01z",
    personoff:"M8.65 5.82C9.36 4.72 10.6 4 12 4c2.21 0 4 1.79 4 4 0 1.4-.72 2.64-1.82 3.35L8.65 5.82zM20 17.17c-.02-1.1-.63-2.11-1.61-2.62-.54-.28-1.13-.54-1.77-.76L20 17.17zm1.19 4.02L2.81 2.81 1.39 4.22l8.89 8.89c-1.81.23-3.39.79-4.67 1.45-1 .51-1.61 1.54-1.61 2.66V20h13.17l2.61 2.61 1.41-1.42z",
    ytlogo:'<svg viewBox="0 0 28 20"><rect width="28" height="20" rx="5" fill="#f00"/><path d="M11.5 5.5v9L19 10z" fill="#fff"/></svg>'
  });
  var O_ic = O.ic, O_esc = O.esc, O_avaStyle = O.avaStyle;   // core primitives

  /* ---------- media helpers (thumb + youtube-flavored avatar) ---------- */
  O.thumb = function (id) { return "https://i.ytimg.com/vi/" + id + "/mqdefault.jpg"; };
  O.handles = {};
  O.avatar = function (n, size, fs) { n=n||"?"; var hd=O.handles[n]; var img=hd?'<img src="https://unavatar.io/youtube/@'+hd+'?fallback=false" onerror="this.remove()" loading="lazy">':""; return '<div class="o-ava" style="'+O_avaStyle(n)+';width:'+size+'px;height:'+size+'px;font-size:'+fs+'px">'+n[0].toUpperCase()+img+"</div>"; };

  /* ---------- PLAYER (IFrame API + captions/translate + PiP) ---------- */
  var P = O.player = {
    yt:null, ready:false, pending:null, cur:null, cc:false, ccLang:"", capFont:2, _state:null, _mini:false
  };
  P.mount = function (hostId) {
    P._host = hostId;
    var t = document.createElement("script"); t.src = "https://www.youtube.com/iframe_api"; document.head.appendChild(t);
    window.onYouTubeIframeAPIReady = function () { P.ready = true; if (P.pending) P._create(P.pending); };
  };
  P._create = function (v) { P.pending = null; P.cur = v;
    P.yt = new YT.Player(P._host, { videoId:v.id, host:"https://www.youtube.com",
      playerVars:{autoplay:1,playsinline:1,rel:0,fs:1,cc_load_policy:(P.cc||P.ccLang)?1:0,cc_lang_pref:P.ccLang||undefined,hl:P.ccLang||undefined},
      events:{ onReady:function(){P._caps(); P._setupGest();}, onApiChange:function(){P._caps();}, onStateChange:function(e){ if(P._state)P._state(e.data); } } }); };
  P.load = function (v) { P.cur = v; if (P.ready && P.yt && P.yt.loadVideoById){ P.yt.loadVideoById(v.id); setTimeout(P._caps,1400); } else { P.pending=v; } };
  P.onState = function (cb) { P._state = cb; };
  P.state = function () { return (P.yt&&P.yt.getPlayerState)?P.yt.getPlayerState():1; };
  P.toggle = function () { if(!P.yt)return; if(P.state()===1)P.yt.pauseVideo(); else P.yt.playVideo(); };
  P.stop = function () { if(P.yt&&P.yt.stopVideo)P.yt.stopVideo(); };
  P._caps = function () { if(!P.yt||!P.yt.loadModule)return;
    try{ if(P.cc||P.ccLang){ P.yt.loadModule("captions"); P.yt.loadModule("cc");
      ["captions","cc"].forEach(function(m){ try{P.yt.setOption(m,"fontSize",P.capFont);}catch(e){} try{P.yt.setOption(m,"reload",true);}catch(e){} try{P.yt.setOption(m,"track",P.ccLang?{languageCode:P.ccLang}:{reload:true});}catch(e){} });
    } else { ["captions","cc"].forEach(function(m){ try{P.yt.setOption(m,"track",{});}catch(e){} try{P.yt.unloadModule(m);}catch(e){} }); } }catch(e){} };
  P.captions = function (o) { if(o.on!==undefined)P.cc=o.on; if(o.lang!==undefined){P.ccLang=o.lang; P.cc=!!o.lang;} if(o.size!==undefined)P.capFont=o.size; P._caps(); };
  P.LANGS = [["","Off (original)"],["en","English"],["es","Espanol / Spanish"],["zh-Hans","Chinese (Simplified)"],["zh-Hant","Chinese (Traditional)"],["hi","Hindi"],["ar","Arabic"],["fr","French"],["ja","Japanese"],["de","German"],["pt","Portuguese"],["ru","Russian"],["ko","Korean"],["it","Italian"],["tr","Turkish"],["vi","Vietnamese"],["th","Thai"],["id","Indonesian"],["nl","Dutch"],["pl","Polish"],["uk","Ukrainian"],["el","Greek"],["iw","Hebrew"],["sv","Swedish"],["fil","Filipino"],["ms","Malay"],["bn","Bengali"],["ta","Tamil"],["ur","Urdu"],["fa","Persian"],["ro","Romanian"],["cs","Czech"]];
  P.mini = function (on) { P._mini = on; document.body.classList.toggle("o-pip", on); };
  P.fs = function () { var f = document.querySelector(".o-player iframe"); if (f && f.requestFullscreen) f.requestFullscreen(); };
  P.gestures = function (onMin) { P._onMin = onMin; P._setupGest(); };
  P._setupGest = function () {
    var pl = document.querySelector(".o-player"); if (!pl) return;
    if (pl.querySelector(".o-gest")) return;                       // once
    var g = document.createElement("div"); g.className = "o-gest";
    pl.insertBefore(g, pl.firstChild);                            // FIRST child: the IFrame API never touches it
    var fs = document.createElement("button"); fs.className = "o-fs"; fs.innerHTML = O_ic("fs"); fs.onclick = P.fs;
    pl.insertBefore(fs, pl.firstChild);
    var sy=0, sx=0, stt=0, drag=false, moved=0;
    g.addEventListener("touchstart", function (e) { var t=e.touches[0]; sy=t.clientY; sx=t.clientX; stt=Date.now(); drag=true; moved=0; }, {passive:true});
    g.addEventListener("touchmove", function (e) { if(!drag)return; var t=e.touches[0], dy=t.clientY-sy, dx=t.clientX-sx; moved=Math.max(moved, Math.abs(dy)+Math.abs(dx));
      if (dy>6 && dy>Math.abs(dx)) { e.preventDefault(); var d=Math.min(dy,320); pl.style.transition="none"; pl.style.transformOrigin="bottom right"; pl.style.transform="translateY("+d+"px) scale("+(1-d/1500)+")"; pl.style.opacity=(1-d/700); } }, {passive:false});
    g.addEventListener("touchend", function (e) { if(!drag)return; drag=false; var dy=e.changedTouches[0].clientY-sy;
      pl.style.transition="transform .2s,opacity .2s"; pl.style.transform=""; pl.style.opacity="";
      if (dy>90) { if(P._onMin)P._onMin(); } else if (moved<10 && Date.now()-stt<300) { P.toggle(); } });
  };

  /* the player widget HTML (host div + minimize + pip overlay). onMin/onMax/onClose are global fn names. */
  O.playerHtml = function (opts) { opts=opts||{};
    return '<div class="o-player" id="'+(opts.id||"o-player")+'">'
      + '<button class="o-min" onclick="'+(opts.onMin||"")+'">'+O_ic("chev")+"</button>"
      + '<div id="'+(opts.host||"o-yt")+'"></div>'
      + '<div class="o-pipov" onclick="'+(opts.onMax||"")+'">'
      +   '<button class="tog" onclick="event.stopPropagation();('+(opts.onToggle||"function(){}")+')()">'+O_ic("pause")+"</button>"
      +   '<button class="cls" onclick="event.stopPropagation();('+(opts.onClose||"function(){}")+')()">'+O_ic("close")+"</button>"
      + "</div></div>"; };

  /* ---------- visual widgets (render HTML) ---------- */
  O.topbar = function (o) { o=o||{};
    return '<div class="o-topbar" id="o-topbar"><div class="o-logo" onclick="'+(o.onHome||"")+'">'+O_ic("ytlogo")+"YouTube</div><div class=\"o-sp\"></div>"
      + '<button class="o-nav" onclick="'+(o.onBell||"")+'">'+O_ic("bell")+"</button>"
      + '<button class="o-nav" onclick="'+(o.onSearch||"")+'">'+O_ic("search")+"</button>"
      + '<button class="o-you" id="o-you" style="background:linear-gradient(135deg,#3ea6ff,#265f94)" onclick="'+(o.onYou||"")+'">Y</button></div>'; };
  O.tile = function (v, onTap) { return '<div class="o-tile" onclick="'+onTap+'(\''+v.id+'\')"><div class="th"><img src="'+O.thumb(v.id)+'" loading="lazy">'+(v.live?'<span class="o-badge lv">LIVE</span>':'')+'</div><div class="tx"><div class="t">'+O_esc(v.t)+'</div><div class="c">'+O_esc(v.ch)+(v.live?' · <span class="o-live">LIVE</span>':'')+'</div></div><button class="o-kebab" onclick="event.stopPropagation();window.'+ (O._kebab||'ok') +'(\''+v.id+'\')">'+O_ic("kebab")+'</button></div>'; };
  O.feedCard = function (v, onTap) { return '<div class="o-fcard" onclick="'+onTap+'(\''+v.id+'\')"><div class="th"><img src="'+O.thumb(v.id)+'" loading="lazy">'+(v.live?'<span class="o-badge lv">LIVE</span>':'<span class="o-badge">HD</span>')+'</div><div class="row">'+O.avatar(v.ch,34,14)+'<div class="tx"><div class="t">'+O_esc(v.t)+'</div><div class="c">'+O_esc(v.ch)+(v.live?' · <span class="o-live">LIVE</span>':'')+'</div></div><button class="o-kebab" onclick="event.stopPropagation();window.'+(O._kebab||'ok')+'(\''+v.id+'\')">'+O_ic("kebab")+'</button></div></div>'; };
  O.chips = function (list, active, onPick) { return list.map(function(c){ return '<button class="o-chip'+(active===c?" on":"")+'" onclick="'+onPick+'(\''+c+'\')">'+O_esc(c)+"</button>"; }).join(""); };
  O.channelRow = function (v, subbed, onSub) { var hh=O.handles[v.ch];
    return '<div class="o-chrow">'+O.avatar(v.ch,36,15).replace('class="o-ava"','class="o-ava" id="o-chava"')
      + '<div><div class="o-chname">'+O_esc(v.ch)+'</div><div class="o-chsub">'+(hh?"@"+hh:O_esc(v.sub||""))+'</div></div><div class="o-sp"></div>'
      + '<button class="o-sub'+(subbed?" on":"")+'" onclick="'+onSub+'()">'+(subbed?O_ic("bellsub")+"Subscribed":"Subscribe")+'</button></div>'; };
  O.actionBar = function (v, s, h) {
    function pill(id,ic,label,on){ return '<button class="o-pill'+(on?" act":"")+'" onclick="'+h[id]+'()">'+O_ic(ic)+(label||"")+"</button>"; }
    return '<div class="o-actions"><div class="o-likewrap">'
      + '<button class="'+(s.liked?"act":"")+'" onclick="'+h.like+'()">'+O_ic(s.liked?"like_f":"like_o")+"Like</button>"
      + '<div class="dv"></div><button class="'+(s.disliked?"act":"")+'" onclick="'+h.dislike+'()">'+O_ic(s.disliked?"dis_f":"dis_o")+"</button></div>"
      + pill("share","share","Share")
      + '<button class="o-pill'+(s.saved?" act":"")+'" onclick="'+h.save+'()">'+O_ic(s.saved?"save_f":"save_o")+(s.saved?"Saved":"Save")+"</button>"
      + pill("remix","remix","Remix") + pill("report","report","Report")
      + '<button class="o-pill'+(s.cc?" act":"")+'" onclick="'+h.captions+'()">'+O_ic("cc")+"Captions</button>"
      + '<button class="o-pill" onclick="'+h.translate+'()">'+O_ic("translate")+"Translate</button></div>"; };
  O.comments = function (id, list, expanded, h) {
    var shown = expanded?list:list.slice(0,1);
    var cl = shown.map(function(c){ return '<div class="o-cmt"><div class="o-ava" style="background:linear-gradient(135deg,#3ea6ff,#265f94)">Y</div><div><div class="w">You · just now</div><div class="b">'+O_esc(c)+"</div></div></div>"; }).join("");
    return '<div class="o-cbox" onclick="'+h.expand+'(event)"><div class="o-chead">Comments <span class="o-ccount">'+(list.length?list.length+" ":"")+'· on this device</span></div><div>'+cl+"</div>"
      + '<div class="o-cghost'+(expanded||list.length?" o-hidden":"")+'"><div class="o-ava" style="background:linear-gradient(135deg,#3ea6ff,#265f94);width:24px;height:24px;font-size:10px">Y</div><div class="f">Add a comment…</div></div>'
      + '<div class="o-crow'+(expanded?"":" o-hidden")+'"><div class="o-ava" style="background:linear-gradient(135deg,#3ea6ff,#265f94);width:24px;height:24px;font-size:10px">Y</div><input class="o-cin" id="o-cin" placeholder="Add a comment…"><button class="o-cpost" onclick="'+h.post+'(event)">Post</button></div></div>'; };
  O.sec = function (label, viewAll) { return '<div class="o-sec">'+O_esc(label)+(viewAll?'<span class="va" onclick="'+viewAll+'">View all</span>':"")+"</div>"; };
  O.ytId = function (q) { var m=String(q).match(/(?:v=|youtu\.be\/|embed\/|shorts\/)([A-Za-z0-9_-]{11})/); if(m)return m[1]; if(/^[A-Za-z0-9_-]{11}$/.test(String(q).trim()))return q.trim(); return null; };
  O.oembed = function (id, cb) { fetch("https://noembed.com/embed?url=https://www.youtube.com/watch?v="+id).then(function(r){return r.json();}).then(function(j){ if(j&&j.title)cb({title:O.strip(j.title),author:j.author_name}); }).catch(function(){}); };
  O.setKebab = function (fn) { O._kebab = fn; };
})();
