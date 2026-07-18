# Handoff: WebView video renders black over the Makepad GL surface

**Symptom.** In the octos app on-device, a `runhtml` web card that plays video
(e.g. the YouTube card) shows the **video area black**. Everything else in the
WebView renders correctly — top bar, thumbnails, the player *chrome* (play/pause,
progress, volume, CC, settings, fullscreen buttons). Only the decoded video image
never reaches the screen.

**This is NOT the app logic.** The card is complete and correct; the video is
actually decoding and "playing". It is a **compositing** problem between the
WebView's video surface and Makepad's GL `SurfaceView`.

## What is proven (don't re-derive)

- **The video plays.** Via CDP against the WebView:
  `octos.player.yt.getPlayerState() === 1` (PLAYING), `getCurrentTime()` advances
  in real time, `getVolume() === 100`, `isMuted() === false`, on a valid **current**
  live id (`VAlMDl00mYY` = Lofi Girl live, confirmed live at handoff time).
- **The video is on a separate surface, not the WebView's HTML layer.** CDP
  `Page.captureScreenshot` — which renders the WebView's *own* compositor — ALSO
  shows the video black, while capturing all the HTML chrome fine. So the `<video>`
  is a punched-through hardware surface, not drawn into the WebView layer.
- **`screencap` (device framebuffer) is black there too** — consistent with a
  hardware/overlay surface the framebuffer read doesn't include.
- **It briefly showed ONLY while a CDP debugger was attached.** Attaching DevTools
  forces the WebView onto a different compositing path and the video appeared;
  detach → black again. This is the key clue: the surface *can* composite, it just
  isn't in the normal overlay path.
- **SurfaceFlinger** (`adb shell dumpsys SurfaceFlinger --list`) shows
  `SurfaceView - dev.makepad.octos_app` (Makepad's GL surface) and **no** separate
  chromium/video SurfaceView layer for the app.

## Architecture (the relevant setup)

- Makepad renders to `MakepadSurface extends SurfaceView` at the **default
  z-order** (bottom; the window is above it). File:
  `makepad/tools/cargo_makepad/src/android/java/dev/makepad/android/MakepadActivity.java`
  (`class MakepadSurface` ~L362; it does NOT call `setZOrderOnTop`/`MediaOverlay`).
- Web cards render in a **native WebView overlay**: `mSystemBrowserOverlay`
  (`FrameLayout`) added to `mRootLayout` above the SurfaceView (~L1382). The WebView
  is created in `ensureSystemBrowser` (~L3260): JS+DOMStorage on,
  `setMediaPlaybackRequiresUserGesture(false)`, **opaque** background
  `setBackgroundColor(0xFF101418)`, a `WebChromeClient` whose `onShowCustomView`
  (~L3272) adds the fullscreen video view to `mRootLayout` (topmost).
- `android:hardwareAccelerated="true"` IS set (confirmed in the built manifest;
  `android/mod.rs` emits it). Device WebView is **Beta 151** (not stock 92).
- The card: `docs/youtube-player-reference.html` composes `octos.player`
  (`aichat/widgets/src/octos_media.js`) over the YouTube IFrame Player API.

## What was tried and did NOT fix it

1. `--disable-features=WebViewSurfaceControl,WebViewThreadSafeMediaDefault` via
   `/data/local/tmp/webview-command-line` **plus a debuggable build** (so the file
   is actually read — the release app is non-debuggable and ignores it). Forced via
   `MAKEPAD_FORCE_DEBUGGABLE=1` (see the env gate added to
   `cargo_makepad/src/android/compile.rs` `let debuggable = … || env`). Video still
   black. (Either the command-line flag isn't the right lever, or SurfaceControl
   wasn't the (only) cause.)
2. An earlier session's note claims the **WebView DevTools flag**
   `WebViewSurfaceControl → Disabled` (the *feature override*, set in the WebView
   DevTools app UI — not the command-line file) fixed it. Could not be re-applied
   over adb (DevTools UI taps don't register via `input tap`), and its efficacy
   this build is UNVERIFIED.

## DECISIVE CLUE (logcat, `--enable-logging=stderr`) — read this first

With `--enable-logging=stderr` in the flag file, the WebView logs confirm:
- **The flag file IS read** (chromium logging appears) — so earlier `--disable-features`
  attempts were applied and still did nothing; they are not the lever.
- **The video decoder runs fine:** `cr_MediaCodecBridge: create MediaCodec video
  decoder, mime video/avc, decoder name OMX.qcom.video.decoder.avc`, `OMX-VDEC-1080P`,
  audio focus granted. So decode + audio work; only the *picture* is missing.
- **The compositor is memory-starved and DROPS content:**
  `[ERROR:cc/tiles/tile_manager.cc:1014] WARNING: tile memory limits exceeded, some
  content may not draw` — spammed continuously. This is the same *class* of error as
  the original black-video bug that `hardwareAccelerated` fixed; it is back.
- `[ERROR:media/gpu/android/frame_info_helper.cc:246] Guessed coded size
  incorrectly. Expected 432x240, got 426x240` (minor).

**Did NOT fix it:** `--disable-features=AndroidSurfaceControl,WebViewSurfaceControl,
UseSurfaceLayerForVideoDefault,UseSurfaceLayerForVideo,OverlayFullscreenVideo`;
`--force-gpu-mem-available-mb=768…1024` (tile error count unchanged, ~82). Even
FULLSCREEN (`onShowCustomView`, top window layer) is black — so it is NOT z-order
occlusion. Everything else (HTML, thumbnails, player chrome) draws.

**Most likely root cause:** the WebView's compositor cannot get enough GPU/tile
memory (or is not truly hardware-accelerated) when hosted as an overlay over
Makepad's GL `SurfaceView`, so `cc` drops the largest layer — the video — while
smaller raster tiles still draw. Best directions to chase in CODE (not flags):
- Force the WebView onto a hardware layer explicitly:
  `web.setLayerType(View.LAYER_TYPE_HARDWARE, null)` in `ensureSystemBrowser`.
- Check the card's rendering resolution: `setUseWideViewPort(true)` +
  `setLoadWithOverviewMode(true)` are set; if the card's `<meta viewport>` isn't
  `width=device-width`, the WebView may raster a huge area → tile-memory blowout.
- Confirm the WebView is actually GPU-composited in this window (the app's main
  content is a GL SurfaceView; the overlay WebView may be getting a starved/soft
  raster budget). Consider a dedicated GPU memory policy / larger tile budget, or
  hosting the WebView so it doesn't contend with Makepad's GL context.
- Sanity-check on a device with more GPU memory / different WebView build — the tile
  budget scales with available GPU memory.

## ROOT CAUSE (SurfaceFlinger layer stack — supporting evidence)

`adb shell dumpsys SurfaceFlinger` while the youtube card plays (audio) shows, for
`dev.makepad.octos_app`, three composited layers, z bottom→top:
1. `Background for -SurfaceView …MakepadApp` (black backdrop)
2. **`SurfaceView - …MakepadApp`** — Makepad's GL surface: `isOpaque=true`,
   `alpha=1.0`, `blend=NONE`, `transparentRegionHint=[0,0,0,0]` (fully opaque),
   `geomBufferSize=[0 0 1080 2215]`, `composition type=DEVICE`.
3. `…MakepadApp#0` — the activity **window** (holds the WebView + overlays), above
   the GL surface.

There is **no video/chromium surface anywhere in the composited stack.** The
WebView's hardware `<video>` renders to a **separate SurfaceView/SurfaceControl
surface** (proven: CDP `Page.captureScreenshot`, i.e. the WebView's own compositor,
shows the video black too — so it is NOT drawn into the WebView layer). That video
surface is a *below-window* surface in the same activity as Makepad's GL surface,
and it is **occluded by / not composited above Makepad's opaque GL SurfaceView** —
so the frames never reach the screen. Decode still runs → **audio only**. Attaching
CDP forces WebView onto a fallback compositing path, which is the only time the
frames appear on-device.

**Not the lever:** `--disable-features=WebViewSurfaceControl` AND
`UseSurfaceLayerForVideoDefault,UseSurfaceLayerForVideo,OverlayFullscreenVideo` were
all set via the command-line file, with a **confirmed-debuggable** app
(`flags=[ DEBUGGABLE … ]`) and a world-readable flag file — video stayed black and
the CDP compositor shot stayed black. So the fix is a **surface z-order / opacity**
problem, not a WebView feature toggle.

**UPDATE — it is NOT occlusion.** Fullscreen (`onShowCustomView` adds the video
view to `mRootLayout`, the top window layer, ABOVE Makepad's GL surface) is **also
black**. So the video surface does not composite at ANY z-order — the problem is
that WebView hardware video never reaches the display in this app at all, not that
it's hidden behind Makepad. Feature flags (`WebViewSurfaceControl`,
`UseSurfaceLayerForVideo*`, `OverlayFullscreenVideo`) on a debuggable build did not
change it (unverified whether the flag file is actually read — `chrome://version`
is blocked in WebView, so the applied command line couldn't be confirmed).

Likely the root issue: the app's **main content is a GL `SurfaceView`** (Makepad),
and a WebView hosted as an overlay in that same window cannot get its hardware
**video** surface composited (audio decodes, video surface is created but never
shown). This is the classic "WebView video is black inside a
SurfaceView/TextureView-based app" problem. Directions to try:
- Confirm whether the command-line flag file is actually read (log
  `--enable-logging=stderr` and grep logcat for the applied switches; or check the
  WebView's variations). If not read, that's why every `--disable-features` no-op'd.
- If SurfaceControl-disable IS applied and video is still black, the WebView video
  needs to render into the WebView's own accelerated **texture** layer. Investigate
  hosting the WebView differently (its own hardware layer / a TextureView-backed
  surface) so video composites with the HTML.
- Verify it EVER worked on-device (the earlier "playing" may have been audio-only —
  lofi is a radio stream). If it never showed video, treat as a first-time bring-up
  of WebView video over a GL SurfaceView, not a regression.

## Hypotheses for Codex (in rough priority)

1. **Z-order between two SurfaceViews.** The WebView's video SurfaceView is likely
   z-ordered *below* Makepad's GL SurfaceView (occluded). Explore making the video
   surface a media overlay above Makepad, or the Makepad window/surface setup so the
   overlaid WebView video composites through. This is the crux.
2. **Fullscreen path may already work (untested).** `onShowCustomView` adds the
   video view to `mRootLayout` at the top. Tapping the player's fullscreen button
   may show video even though inline doesn't — if so, that both confirms the
   diagnosis and offers a usable path (play video large/fullscreen), and points at
   the inline fix.
3. **Correct WebView feature to disable.** Verify the exact base::Feature that
   controls the SurfaceControl video path on WebView 151 (the DevTools override
   worked before; the command-line `WebViewSurfaceControl` did not — names may
   differ). Consider `--disable-features` combos or the `WebViewSurfaceControlFor...`
   variants.
4. **Opaque WebView background** (`0xFF101418`) over a punched-through video surface
   could be occluding it; test a transparent WebView + explicit z handling.

## Repro

```
ADB=~/Library/Android/sdk/platform-tools/adb   # device bf0a4730 = OnePlus 6T
$ADB -s bf0a4730 install -r <apk>
$ADB -s bf0a4730 shell "am start -S -n dev.makepad.octos_app/.MakepadApp --es makepad.AUTO_PROMPT 'youtube'"
# full app renders; video area is black.
# confirm it IS playing (WebView devtools):
$ADB -s bf0a4730 forward tcp:9222 localabstract:$(… webview_devtools_remote_<pid>)
#   octos.player.yt.getPlayerState() -> 1 ; getCurrentTime() advances
```

Note: `screencap` cannot capture the video surface, so verify visually on the
physical device (or via the player API), not via screenshots.

## State of the tree at handoff

- **FU (done):** youtube routes now serve `docs/youtube-player-reference.html`
  DIRECTLY (no LLM) with fresh live ids — `route_to_app` in `app/app/src/main.rs`
  (`youtube_reference_card()` + `YOUTUBE_REF_PLACEHOLDER_IDS`). The full app renders
  reliably; only the video-compositing above remains.
- **Uncommitted dev hack:** `cargo_makepad/.../compile.rs` has a
  `MAKEPAD_FORCE_DEBUGGABLE` env gate (to read the WebView command-line file). Build
  with `MAKEPAD_FORCE_DEBUGGABLE=1 cargo makepad android build -p octos-app --release`
  after `cargo install --path makepad/tools/cargo_makepad --force` (build the tool
  with `RUSTFLAGS=""` to avoid the box3d profdata error).
