# Aster whole-app performance audit — alpha.25

Measured on the actual media center, 2026-11-16: Intel Core i5-8500T, UHD 630, Arch Linux, KDE Bigscreen on Wayland, 1920×1080 app window. CPU percentages below use **one logical core as 100%**, not the whole CPU. This is a targeted engineering audit with short repeatable samples, not a full release certification.

## Findings and fixes

1. **The entire UI repainted continuously while idle.** Replace the unconditional 16 ms repaint loop with explicit wakeups for input, asynchronous results, short transitions and media frames. Controller detection sleeps longer without a connected pad; the inspection socket blocks instead of polling. Active animation is paced without re-enabling the previously diagnosed Wayland swap-wait stall.
2. **The player overlay rebuilt unchanged frames.** Update ASS only when its content/size changes, publish state only when changed, and stop its timer when hidden or paused and stable. Input, resize, tracks, volume and seek completion wake it again.
3. **Each decoded frame caused two UI paints.** Coalesce render notifications with a nonzero repaint deadline. The callback only records pending work and unparks a notifier, avoiding locks or mpv calls in that callback.
4. **The media center lacked Intel's VAAPI driver.** Install private, checksum-verified official Arch `intel-media-driver 26.2.4-1` and `intel-gmmlib 22.10.0-1` under Aster's runtime directory. No global package changes. The launcher selects these only for Aster.
5. **Copy-back decoding was especially expensive at 4K.** Supply the native Wayland/X11 display to libmpv's render API and select `auto-safe` hardware decoding. On this Intel system the final path is direct `vaapi`, instead of downloading and re-uploading frames through `vaapi-copy`. Unsupported systems retain mpv's fallback selection.
6. **Two virtual keyboards could compete.** Keep Aster's custom keyboard cursor steady and suppress native IME activation only while that keyboard is open. Its idle load now reaches zero. Ordinary text input remains available.
7. **Artwork outside the viewport still did work.** Cull offscreen card artwork/text/focus painting while retaining layout and focus-reveal logic. Thumbnail retrieval/decoding remains off the UI thread, with a three-sheet cache and debounced JPEG fallback snapshots.

## Measured results

Baseline is installed alpha.24; optimized results are alpha.25. Four-second samples follow settling. The 360-card fixture uses one cached artwork atlas; optimized Discover exercises six shelves, whereas alpha.24 ignores the extra fixture shelf description. Thus Discover is a before/after workload indication rather than an identical layout comparison. Navigation workloads were corrected to move through actual cards in the optimized run; their old/new CPU values are deliberately not presented as a controlled comparison.

| View / activity | Baseline CPU | Optimized CPU |
| --- | ---: | ---: |
| Sign-in idle | 86.7% | 0.25% |
| Sign-in custom keyboard idle | 97.4% | 0% |
| Discover idle | 100.1% | 0% |
| Movies idle | 104.4% | 0.25% |
| Series idle | 104.9% | 0% |
| Settings idle | 98.4% | 0.25% |
| Search keyboard idle | 95.6% | 0.25% |
| Paused 1080p video | 112.3% | 0.25% |
| Chapter picker paused | Not implemented | 0% |
| Audio / subtitle menu paused | Not sampled | 0% |
| 1080p AVC playback | 72.5%, software decode | 9.7%, direct VAAPI |
| 4K HEVC 10-bit, copy path vs final direct path | 125.4%, VAAPI copy | 9.7%, direct VAAPI |

The final 4K sample rendered at 24.15 fps for a 24 fps source, with zero decoder-dropped or VO-delayed frames reported. The copy path managed only about 17–20 UI paints/sec despite no decoder drops, so decoder counters alone were insufficient to establish smooth presentation. Pausing the final 4K path reduced UI/render FPS and GPU engine activity to zero.

Active card navigation remains animated at about 61 UI fps. In the final run, navigation used 27–34% of one core and search updates used 20%. The playing timeline with decoded-snapshot previews was a higher transient workload (42%); paused thumbnails and chapter/track menus sleep. These are active-work samples, not idle leaks. Hardware sensors reached the low 40s °C in the optimized runs; starting temperature and run order varied, so this is not a controlled thermal comparison. Fan RPM/acoustics were not measured.

## Coverage and limits

Runtime coverage: sign-in, keyboard, Discover, Movies, Series, Settings, search updates, card navigation, hidden/visible playback controls, seeking previews, pause, chapter/audio/subtitle menus and fullscreen. Source review also covered async library/auth responses, inline season/episode fetching and stale-response handling, pagination, controller polling, preview caching and render-thread ownership. Existing regression checks cover inline season browsing, resume, episode boundaries/autoplay and progress reporting.

The benchmarks use synthetic media and a loopback mock, never a real title or real watch history. They do **not** measure Jellyfin server/network latency, a cold cache of thousands of distinct artwork images, long-duration memory retention, physical controller radio behavior, every codec, HDR tone mapping or 4K60. The general egui artwork cache still warrants a long-session memory test. A short test passing is not proof that every library or display path is performant.

## Reproduce and evidence

From the source directory, with a graphical desktop and libmpv available:

```sh
ASTER_AUDIT_MEDIA=/absolute/path/to/synthetic-1080p.mp4 \
ASTER_AUDIT_HWDEC=1 ASTER_AUDIT_MODERN=1 ASTER_WINDOW_INACTIVE=1 \
python tests/performance_audit.py /absolute/path/to/aster /tmp/aster-audit
```

Use `ASTER_AUDIT_LOGIN=1` for sign-in/keyboard and `ASTER_AUDIT_ONLY_PLAYBACK=1` for a codec probe. `/proc` CPU/RSS and DRM engine counters are sampled alongside safe mpv decoder properties. The tool writes `performance.json` and `app.log`; it creates its own private profile, media server and fixture. It closes the app/server when done. CPU is app-only; the Python fixture server is not counted.

Engineering workspace evidence is in `work/performance-results/`, `work/aster-final-tests.log`, `work/aster-final-clippy.log`, and the `work/aster-native-display-final/` GPU captures. Media-center originals are under `~/.local/state/aster/audit/`. These logs contain synthetic data, not connection credentials.

Verified: 44 Rust tests, strict Clippy, and graphical integration checks for in-window rendering, chapter picker/markers and metadata fallback, trickplay and decoded snapshots, pause/resume, seeking acceleration, mouse controls, tracks/Off, resize/fullscreen, repeat playback, previous/next, EOF/autoplay and shutdown. Controller commands were injected through the same action path; this does not substitute for a hands-on controller session.

## Installation and rollback

Alpha.25 uses the existing private matching libmpv/FFmpeg runtime and a private Intel driver directory. Settings and saved credentials are preserved. Earlier releases and the previous launcher backup remain under `~/.local/share/aster/releases/` and `~/.local/state/aster/launcher-before-alpha25-*`. Restore that launcher to roll back; do not delete the shared alpha.16 runtime while later releases reference it. Generic public installation does not force Intel environment variables.

API references: [mpv render API](https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h) documents callback restrictions and native display parameters; [mpv input commands](https://github.com/mpv-player/mpv/blob/master/DOCS/man/input.rst) documents screenshot output behavior.
