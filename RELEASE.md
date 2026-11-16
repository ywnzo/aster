# Release status and remaining product work

## 0.1.0-alpha.27 — CI releases and desktop icon refresh

- Test formatting, Clippy, all Rust tests and both embedded player preview modes on GitHub Actions. Publish checksummed Linux x86_64 downloads after matching version tags pass.
- Give the transparent white-star desktop icon a fresh icon name and refresh KDE desktop services during installation to avoid stale icon caches.

## 0.1.0-alpha.26 — transparent star icon

- Remove the app icon background and border. Use the same white star on transparency for the launcher, window icon and header.

## 0.1.0-alpha.25 — chapter picker and whole-app performance

- Y during playback (C on a keyboard) opens Chapters. Up/Down selects a chapter; A seeks precisely while preserving pause. The chapter button is also available in the player controls. B still stops and returns.
- Replace continuous app redraws with explicit input, worker-result, animation and video-frame wakeups. Sleeping menus, catalogs, settings and the custom keyboard no longer burn a CPU core. Preserve the verified Wayland swap-wait workaround.
- Deduplicate unchanged player overlay updates, sleep its paused/hidden timer, cull offscreen artwork and prevent KDE from opening a second virtual keyboard over Aster's keyboard.
- Keep preview fetch/decode work off the UI thread and use bounded, debounced JPEG snapshots for fallback previews.
- Audit sign-in, keyboard, Discover, Movies, Series, Settings, search, navigation, track/chapter menus, thumbnails, paused/fullscreen playback and decoder behavior on the media center. See [PERFORMANCE-AUDIT.md](PERFORMANCE-AUDIT.md) for measurements and limits.

## 0.1.0-alpha.24 — timeline chapters

- Draw chapter boundaries on the timeline, support clicking a marker to seek, and show the current/hovered chapter title while navigating the timeline.
- Preserve embedded chapters and fill missing chapters from the existing authenticated Jellyfin metadata request. Discard metadata for an episode that has already been replaced.

## 0.1.0-alpha.23 — seek previews and stable library scrolling

- Show timestamped seek thumbnails from authenticated Jellyfin trickplay tiles; use the current decoded video frame when trickplay is unavailable. Fetch and decode off the UI thread, cache up to three sprite sheets, and clean private snapshot files when the worker exits.
- Keep vertical wheel input vertical in nested shelves. Reveal only a clipped card edge instead of recentering the row; isolate parent vertical focus scrolling from sibling horizontal rows. Apply the same behavior to seasons and episodes.

## 0.1.0-alpha.22 — accelerated timeline seeking

- Held timeline directions use 10-second steps, then 30 seconds after 1.2 seconds and 60 seconds after 3 seconds. Single taps remain ten seconds.
- Explicit controller and keyboard repeat/release events reset acceleration on release, reversal, focus loss and leaving the timeline. Display the current seek step next to the focused timeline.

## 0.1.0-alpha.21 — controller timeline navigation

- Up focuses and highlights the timeline; left/right seeks ten seconds. Down restores the last player button, where left/right resumes moving focus without seeking.
- Timeline navigation preserves pause state and volume. Track-menu navigation retains its own Up/Down behavior.

## 0.1.0-alpha.20 — keyboard space and player focus

- Y inserts a space while the keyboard is open; L2 / LT remains the symbols shortcut.
- Left/right opens and moves focus across playback controls instead of seeking. Seeking remains available through the rewind/forward buttons and timeline.

## 0.1.0-alpha.19 — keyboard shortcuts

- R2 / RT toggles Shift, L2 / LT toggles special characters, and Start / Menu accepts the text. X continues to delete; browsing and playback retain their existing X / Start behavior.
- Show controller shortcuts inside the keyboard and in the footer.

## 0.1.0-alpha.18 — episode switching and Bigscreen input

- Player Previous/Next buttons, LB/RB shortcuts and Page Up/Page Down switch episodes in the same window, across season boundaries. Manual navigation works with autoplay off and sends separate stop/start reports. Boundaries leave the current episode open.
- Prevent duplicate navigation from native gamepad input plus Bigscreen’s synthesized keyboard events. Controller confirmation no longer also reaches the on-screen keyboard as Return. Global keyboard navigation is consumed before focused widgets can activate again.
- Recognize controllers from mapped controls rather than cached button state, so the Xbox pad is usable immediately after connection.
- Stop routing gamepad input to an unfocused player.

## 0.1.0-alpha.16 — translucent navigation bars

- Header and controller footer use a dark 76% opaque surface with native window alpha; labels and the white logo stay opaque. Main content and playback remain opaque.
- Consecutive video clicks also handle mpv’s double-click event so pause/resume stays responsive.

## 0.1.0-alpha.15 — playback controls

- B / Back and the top-left arrow stop playback and return immediately, including from player menus and controls.
- Removed the redundant bottom Stop button and updated controller focus wrapping.
- Clicking the video toggles pause/resume; timeline, volume, menu and transport clicks retain their own actions.

## 0.1.0-alpha.14 — Aster branding

- Renamed the app, executable and packaging to Aster, with a shared curved star mark for the header and application icon.
- Existing Hearth preferences and saved connections remain readable without a new sign-in.

## 0.1.0-alpha.13 — compact library toolbar

- Library heading and count share one row. Toolbar height is reduced from 84 to 42 logical pixels including padding.
- Filter, sort and view controls use content-sized flat labels, small dropdown chevrons and hover-only backgrounds instead of large filled boxes.
- Disabled client-side swap waiting on Wayland after reproducing an EGL SwapBuffers stall that blocked the UI during startup. Aster keeps its frame scheduling and the compositor handles presentation; X11 behavior is unchanged.

## 0.1.0-alpha.12 — embedded libmpv and full-bleed artwork

- Replaced the separate mpv process/window with libmpv and an OpenGL render callback inside Aster's existing client area. The library header/footer disappear during playback; the same player overlay, controller bindings and mouse controls render over the video.
- Resume, external subtitles, track menus, watch-progress reports, natural next-episode autoplay, Play now/Cancel and Stop remain on the existing playback worker. Stop returns to the same app view, and the embedded core can start another title without recreating a window.
- Fullscreen now changes Aster's window. GL resources are released on the UI context before the libmpv core is destroyed.
- Removed outer side gutters across media tabs and detail backdrops. Text, toolbars and detail-body controls retain interior padding.
- The embedded OSD surface remains alive across natural EOF so Up next is visible before the next file starts. Subtitles move above the transport strip while controls are visible.
- All 33 Rust tests and strict Clippy pass. The standalone generated-media GL smoke test verifies in-window rendering, no child player, mouse/controller controls, audio/subtitle selection, resume, resize, fullscreen, Stop/replay, visible natural-EOF Up next/Play now, and clean shutdown during playback.
- Runtime now requires libmpv; Debian build instructions include libmpv-dev. The OpenGL path uses hardware copy-back decoding for compatibility; direct GPU surface interop and HDR remain release checks.

## 0.1.0-alpha.11 — resume playback fix

- Fixed mpv resume initialization: its start option requires a time string; numeric IPC values were rejected and playback silently began at zero.
- A native regression test checks actual post-load playback position for a saved offset, Resume disabled, and no saved progress. It uses local synthetic media and does not change the user's server.
- All 33 tests, including native streaming/autoplay/resume, and strict Clippy pass.

## 0.1.0-alpha.10 — aligned branding and compact player transport

- Replaced the house icon with an H/play monogram. Header branding and navigation share the same 32-pixel row geometry, avoiding nested navigation alignment offsets.
- Player startup follows the windowed/fullscreen preference.
- Native player overlay uses a small back/title row and a translucent 82-pixel bottom strip, with a scrubber, elapsed/remaining time, compact play/pause and ten-second skip icons, mute/volume, audio, subtitles, speed, fullscreen and stop controls.
- Controller focus and mouse hover reveal control labels; audio/subtitle menus and Up next remain available. Narrow windows hide the volume slider while retaining mute and controller volume adjustment.
- All 32 tests pass, including native synthetic streaming and natural-completion autoplay. The separate native control/rendering test and strict Clippy pass. Header alignment was inspected in a native fixture.

## 0.1.0-alpha.9 — logo and compact control hints

- Header uses Aster's house/play logo instead of plain text, with an accessible Home label and click-to-Discover behavior.
- Footer is a slim dark hint strip with small outlined controller letters and lightweight labels, replacing filled keycap boxes and oversized padding.

## 0.1.0-alpha.8 — distinct focus and contained progress

- Focus outlines use a cool blue, distinct from the peach playback-progress color.
- Thumbnail progress bars are inset from all rounded edges, with rounded ends and a dark track. Dashboard and catalog cards share the same geometry.

## 0.1.0-alpha.7 — cleaner controls and unclipped rows

- Header is a compact single row at supported desktop widths, with reduced padding, smaller branding and tighter navigation.
- Development-bridge actions cannot toggle watched/favorite flags or start real playback; native user input remains unaffected.

- Focus outlines now animate inside their target bounds instead of drawing multiple expanding outer rings. Horizontal rows have a small leading inset.
- Season selectors use simple text tabs with a thin active underline and subtle focus tint.
- Standard buttons use restrained dark fills, smaller corner radii and a single inset focus outline. Default button borders are removed.
- Horizontal scrollbars are hidden on all horizontal rows; controller navigation, wheel/trackpad scrolling and automatic focus scrolling remain available.

## 0.1.0-alpha.6 — inline seasons and motion

- Windowed startup is now the default and is selected in the current development profile; remembered sign-in stays enabled.

- Series detail pages now show season selectors and episode thumbnails above Cast & Crew. Choosing a season stays on the page; episode details return to the selected season/episode and preserve the original library focus.
- Added short eased page fades and animated focus rings/glows on catalog cards, dashboard shelves, detail actions, season selectors, episode cards and ordinary controls. Focus targets stay stable during animation.
- Controller Down moves from hero actions to seasons, then episodes. Left/Right stays within each row, A selects, B returns to hero actions or the previous detail page. LT/RT scroll the page.
- Async episode responses are checked against the current series and selected season. Autoplay updates the inline selection when crossing seasons.
- All 31 tests pass, including native synthetic autoplay and streaming tests; strict Clippy passes. Signed-in inline browsing and native controller-action navigation are checked on KDE. Physical controller hardware remains a separate release check.

## 0.1.0-alpha.5 — cinematic dashboard and title pages

- Rebuilt movie and episode details around large backdrops, a compact poster/title strip, playback/favorite/watched actions, synopsis, genre tags and cast portraits from Jellyfin metadata. Episodes use their own landscape artwork and the series poster.
- Replaced Discover's generic catalog with a featured-title carousel and independent horizontal Continue Watching, Next Up, latest movie/show and genre shelves. Featured logos display when available.
- Controller navigation stays within each shelf, moves between uneven rows, scrolls selected cards into view, and reaches the featured carousel. Back restores the previous shelf/card.
- Verified the signed-in native screens against the supplied design references at TV and compact sizes without playing or changing real media. All 27 regular tests pass, including uneven-shelf/controller-carousel navigation and returning to Discover while its data reloads; strict Clippy passes. Native playback tests remain available separately.

## 0.1.0-alpha.4 — seasons and next-episode autoplay

- Series → Seasons → Episodes browsing uses Jellyfin's dedicated season/episode endpoints, including paging, episode counts, artwork, numbered landscape episode cards and watched/resume indicators.
- Back restores the selected episode, season and original series card. All episodes remains available from the season view.
- Natural episode completion prepares the next episode in the same native player window. A themed Up next card offers Play now and Cancel autoplay by controller or mouse.
- Autoplay defaults to on with a seven-second countdown; both are configurable under Playback. Normal episodes cross season boundaries without inserting specials; explicitly selected specials stay within specials.
- Stop, Back, playback errors, movies, disabled autoplay and the end of a series never start another episode. Next episodes start at zero, with individual start/stop progress reports.
- Verified the real signed-in server's season and episode screens without playing or modifying its media. All 27 Rust tests pass, including native mpv autoplay/Play now/cancel/disabled/error and authenticated streaming/progress tests. Strict Clippy and the separate native overlay rendering/control test pass.

## 0.1.0-alpha.3 — library browsing and playback menus

- Remember sign-in and fullscreen are on by default and enabled in the active development profile. A fresh launch restored the saved keyring session and loaded the library without another login.
- Search is now a wide, editable field. Featured banners belong to the library scroll area and scroll fully out of view.
- Movies/Series use compact Fathom-style poster grids, ratings, title sorting, watched/favorite filters, release-year/date-added/rating sorting, and grid/list views. Filter/sort options are sent to Jellyfin so pagination stays correct.
- The native video window has a themed control overlay: seeking, play/pause, volume, speed, fullscreen and stop.
- Audio/subtitle menus show real track names, languages, codec/channel/sample-rate information, default/forced/external flags, and a subtitles Off option. Controller navigation works inside these menus.
- Text-based external Jellyfin subtitles load with authenticated headers; credentials are excluded from their URLs, and cross-server subtitle URLs are rejected.
- Checked against the real signed-in library: 156 movies (120 loaded initially), watched filter returning 12 titles, release-year sorting, list view, and all 29 series.
- Twenty-two regular Rust tests pass, plus the explicit native streaming/external-subtitle/watch-progress test. The reusable native-controls test passed with two audio and two subtitle tracks, including selection, Off, speed and Back handling.

The app is still a development alpha. Physical controller hardware, the media-center GPU/codec combinations, and clean-machine packaging remain release checks.


## 0.1.0-alpha.2 — layout and input redesign

- Inspected the original signed-in window directly; reproduced the hero being scrolled out of view and crowded card layout.
- Split the fixed featured area from the independently scrolling, virtualized catalog.
- Added aspect-preserving artwork crops, bounded two-line card titles, adaptive card sizing, and small progress/watched indicators.
- Reworked the navigation header, detail page, contextual controller hints, and grouped settings.
- Replaced the sign-in page with a centered card and ordinary editable text fields; the on-screen keyboard is explicitly requested rather than forced on mouse clicks.
- Replaced the flat ASCII keyboard with a controller-navigable QWERTY keyboard, separate symbols page, Shift, wide Space, Delete and Done.
- Quick Connect has a separate compact approval screen.
- Added native visual checks at compact, desktop and 1080p TV sizes, plus headless 4K layout checks and sign-in-field geometry regression coverage.
- Added opt-in private local development control for inspecting native frames and handing sign-in between development builds in memory, without persisting credentials. Disabled in normal launches.

The new build is open for sign-in. Its library layout was checked with a visual fixture using artwork from the inspected original window; authenticated browsing in the redesigned build still requires a new sign-in. Physical gamepad verification remains pending.


## 0.1 development alpha

The first usable native implementation is present. A successful build and automated
tests are evidence for the checked code paths, not a claim of production readiness.

Verified during development:

- Native app launched and rendered on this Arch/KDE desktop.
- Sign-in, settings, and the controller keyboard captured from their own framebuffer and visually inspected.
- Cargo check, 20 automated protocol/navigation/storage/layout/keyboard tests, and strict Clippy have passed.
- An additional native integration test used real mpv and a generated silent video to verify authenticated HTTP streaming, IPC pause/seek, natural completion, and start/stop watch-progress reports.
- Release archive contents/checksum, desktop entry, CLI, and installation into an isolated prefix verified.
- Existing Jellyfin 10.11.6 at the user's LAN address responds to public system info.
- A Quick Connect code could be requested. Authenticated real-library testing is pending.

Not yet verified:

- Full authenticated login/browsing/playback against the user's library.
- Physical controller input, controller sleep/reconnect, and Bigscreen input conflicts.
- Real video codec combinations, hardware decoding, HDR, passthrough, audio/subtitle switching.
- Keyring behavior on the media center, private-file persistence across a real restart.
- Kiri/Debian build path, clean-machine installation, multi-monitor/HiDPI behavior.

## Required before public distribution

1. **Playback completeness:** playback session/source IDs; negotiated transcoding and bitrate limits;
   stream cancellation and robust progress reporting on connection failures.
2. **Library completeness:** selectable libraries,
   profile switching, server discovery, multiple connections,
   reconnect/retry UI, and bounded artwork caching.
3. **TV usability:** hardware-tested Xbox/PlayStation layouts; remapping beyond A/B;
   analog/D-pad thresholds and repeat calibration; controller-only login/error recovery;
   focus persistence across every route; Bigscreen desktop input arbitration;
   subtitle readability and couch-distance testing at 1080p and 4K.
4. **Visual finish:** stronger cinematic home composition, smooth route transitions,
   consistent iconography, responsive detail layouts, accessible text/focus/contrast,
   and a polished player overlay. Current egui layout is the first implementation.
5. **Settings/product:** language, theme/accent selection, accessibility/reduced motion,
   playback language/subtitle defaults, diagnostics with redaction, config migration,
   and clear offline/expired-account handling.
6. **Distribution:** settle the app name, confirm licensing and dependency notices,
   establish repository/CI and signed release artifacts, build on a suitable baseline,
   validate clean Arch and Debian machines, add Flatpak/AppImage packaging and updates,
   and test uninstall without touching user media or other applications.

No Git repository has been published, no remote media-center software replaced,
and no automatic startup configured.
