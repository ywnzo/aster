# Aster

A native Rust Jellyfin client designed around a gamepad, a television, and a sofa.
Aster is an independent implementation, with a dark, peach-accented interface
inspired by Fathom and TV navigation inspired by Switchfin. No code or artwork
has been copied from either project. The name is provisional.

**Status: development alpha, not a finished public release.** See [RELEASE.md](RELEASE.md).

## Implemented

- Jellyfin password login and Quick Connect approval from another signed-in app.
- Discover, movies, series, continue watching, favorites, search, and paginated browsing.
- Cinematic movie/episode details with full-width backdrops, compact actions, genre tags and cast portraits.
- Short page fades and animated inset focus outlines across catalogs, shelves and detail actions.
- Minimal season tabs and button styling, with horizontal scrolling kept free of visible scrollbars.
- Discover dashboard with featured titles and horizontal Continue Watching, Next Up, latest movie/show and genre shelves.
- Inline season selection and landscape episode rows above Cast & Crew on series details, with favorite toggles, and watched/unwatched toggles.
- Compact centered sign-in, direct desktop text entry, and a QWERTY/symbol on-screen keyboard.
- Windowed startup by default, with remembered sign-in enabled. Fullscreen remains available in Settings or with F11.
- Aspect-preserving artwork, compact Movies/Series grids with ratings, alphabetic sorting, watched/favorite filters, list view, and grouped settings.
- Wide editable search and featured banners that scroll away with the catalog.
- Native gamepad polling, D-pad and analog navigation, focus outlines, held-input repeat,
  reconnect detection, button hints, swapped confirmation buttons, adjustable deadzone.
- Controller-operated on-screen keyboard with upper/lowercase, digits, punctuation,
  delete, space, and support for ordinary keyboard/IME input.
- In-window libmpv/OpenGL direct-stream playback with resume, hardware decoding, controller
  pause/seek/volume and named audio/subtitle menus, and Jellyfin watch-progress reporting.
- Next-episode autoplay across seasons, with Play now/Cancel controls and a configurable countdown.
- Automatically saved display, playback, controller, and credential preferences.
- Optional saved sign-in using the system keyring or an explicitly selected private file.
- Linux desktop launcher, original SVG icon, packaging and Kiri container build scripts.

## Run on Arch Linux / KDE

Requires Rust 1.88 or newer, a working OpenGL driver, `libmpv` (included in Arch's `mpv` package), and system libraries
for Wayland/X11 and libudev. `secret-tool` from `libsecret` is needed only for
system-keyring storage. On a typical Arch KDE installation, install development
requirements with:

```sh
sudo pacman -S --needed rust base-devel pkgconf systemd-libs mpv libsecret
cargo build --locked --release
./target/release/aster --windowed --server http://YOUR_SERVER:8096
```

Aster is a native application; it starts no HTTP development server. Any future
web/dev services must use Kiri. To build in Kiri against Debian Bookworm instead
of the host's newer glibc:

```sh
./scripts/kiri-build
./scripts/package ./dist/kiri/aster
```

This build path is included but has not yet been executed. The current binary
is built on Arch and should first be tested on the user's Arch machines.

Video is rendered directly into Aster's existing window; no external player process or second window is launched. Stop returns to the same detail/library view. Fullscreen applies to the Aster window. On Debian, building requires `libmpv-dev` and running requires `libmpv2`. Hardware decoding imports video surfaces directly where supported, with automatic fallback. Intel UHD 630 direct VAAPI rendering has been verified; device-specific HDR/passthrough still require hardware validation.

## Controls

| Input | Browsing | Playback |
|---|---|---|
| D-pad / left stick | Move focus | Left/right: focus buttons; Up: timeline; Down: buttons |
| A / South | Select | Pause/resume |
| B / East | Back / cancel | Stop immediately and return |
| X / West or Start | Settings | Focus playback controls |
| Y / North | Search | Show progress |
| LB / RB | Previous/next section | Previous/next episode |
| LT / RT | Filter / sort | Open audio / subtitle menus |

A/B can be swapped in Settings; the physical face-button layout remains the
same for PlayStation and other controllers supported by gilrs. A real controller
and its sleep/reconnect behavior still need hardware validation.

Keyboard: arrows, Enter, Escape, F1 settings, F3 search, F11 fullscreen.
During in-window playback: Space pauses, Tab focuses controls, A opens audio, S opens subtitles, Page Up / Page Down switch to the previous / next episode, and Escape stops and returns. Click the video to pause/resume; the top-left arrow also stops and returns. Mouse users can scrub the timeline and adjust volume directly. Track menus show language, codec, channel layout, sample rate, and default/forced/external flags when provided by the media. Text-based external Jellyfin subtitles are loaded with authenticated headers.

Series details include numbered season selectors and episode thumbnails above Cast & Crew. Press Down from the hero actions to focus seasons, Left/Right to choose, A to load a season, then Down for episodes. Episode details return to the same series, season and episode with Back. LT/RT scroll the detail page.

Autoplay is enabled by default with a seven-second countdown. Settings → Playback lets you disable it or change the delay (0–15 seconds). Only a natural episode ending starts the countdown; Stop, Back, errors and movies do not autoplay. Normal episodes continue across season boundaries without inserting specials. The next episode starts from the beginning.

## Settings and credentials

Preferences: `$XDG_CONFIG_HOME/aster/settings.json`, defaulting to
`~/.config/aster/settings.json`. Preferences never contain passwords or tokens.
The password is used for sign-in and not saved.

Remember sign-in is on initially. With **System keyring**, tokens go through
`secret-tool` to a running Secret Service provider such as KWallet or GNOME
Keyring. With **Private file**, the token is unencrypted in
`$XDG_DATA_HOME/aster/session.json` (default `~/.local/share/aster/session.json`),
with a `0700` parent directory and `0600` file permissions. Select this explicitly
if no keyring is available. Turn off remembering before changing storage modes.
Sign out removes the remembered token from the selected backend.

The app sends tokens in authentication headers, including to the embedded libmpv core via a private
Unix socket. Tokens are not embedded in stream/artwork URLs or process arguments.
HTTP is supported for trusted LANs; use HTTPS across untrusted networks.

## Install and package

```sh
./scripts/install-local
./scripts/package
```

Installation is per-user, into `~/.local/bin` and `~/.local/share`. Add
`~/.local/bin` to PATH for the desktop launcher. A portable archive contains the
binary and installation files; after extraction run
`./scripts/install-local ./aster`. No autostart or existing media client is changed.
`libmpv.so.2` and the system graphics/input libraries remain runtime dependencies.

The current source and generated binary are for Linux. Windows/macOS support is
not implemented: playback IPC and credential-file permissions currently use Unix APIs.

## Development verification

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
# Headless libmpv streaming/resume/autoplay checks are included in cargo test.
# Actual in-window GPU/controller smoke test (generated media only):
python3 tests/embedded_ui.py /absolute/path/to/aster /tmp/aster-gl-check
# Optional standalone Lua overlay regression; this test alone uses the mpv executable
python3 tests/native_controls.py
```

API tests start an ephemeral localhost fixture, never the user's server. Navigation
tests exercise gamepad-equivalent actions without a display. Developer window
captures use `ASTER_CAPTURE_PATH=/absolute/output.png` and optionally
`ASTER_CAPTURE_SCREEN=settings`; the latter uses an explicitly fake connection.
Capture mode closes the window after saving its own framebuffer.

## Design references

- [Fathom](https://github.com/TraceApps/fathom)
- [Switchfin](https://github.com/dragonflylee/switchfin)
- [egui / eframe](https://github.com/emilk/egui)
- [gilrs](https://docs.rs/gilrs/0.11.2/gilrs/)
- [mpv IPC](https://mpv.io/manual/stable/#json-ipc)

### Native UI inspection during development

`ASTER_DEV_BRIDGE=/private/directory/control.sock` enables an opt-in Unix
socket for native screenshot capture, view/focus actions, and non-secret geometry
status. The directory is user-only (`0700`), and the socket is `0600`. A later
build can use `ASTER_DEV_RESTORE=/private/directory/previous.sock` to hand off
its session directly in memory. This never writes a token to a file and is off
in ordinary launches. Stale source sockets fall back to normal sign-in.

Visual fixtures are explicitly selected with `ASTER_PREVIEW_DATA`; local
artwork atlases and fixture data stay in `work/` and are not included in packages.

## Embedded playback verification

`cargo test` includes headless libmpv tests for authenticated streaming, external subtitles, resume, natural completion, autoplay and cancellation. They use local mock servers and synthetic clips.

For the actual GPU window, mouse/controller commands, resize, fullscreen, repeat playback and active-playback shutdown, run:

```sh
python tests/embedded_ui.py /absolute/path/to/aster /tmp/aster-gl-check
```

This needs Python 3, ffmpeg, libmpv and a graphical desktop. It generates its own video/artwork, starts an ephemeral loopback mock, and never signs in to or modifies a real Jellyfin library. Captures and a report remain in the optional artifact directory.

Existing Hearth settings and saved sign-in are read automatically when no Aster profile exists. New preferences and connections use the Aster namespace.

On KDE Bigscreen with a native gamepad connected, Aster filters Bigscreen’s mirrored navigation keys inside its own window to avoid handling each controller press twice. Typed text and pointer input remain available; Bigscreen’s controller service is not changed.

On the on-screen keyboard: R2 / RT toggles Shift, L2 / LT toggles symbols, Y inserts a space, X deletes, and Start / Menu accepts the entered text. A types the highlighted key and B cancels (subject to the A/B swap setting).

Left/right opens and moves focus through playback controls without seeking. Press A to activate the focused control; the rewind/forward buttons still seek by ten seconds when activated.

Press Up during playback to focus the highlighted timeline. Left/right then seeks in ten-second steps; Down restores the previously focused player button.

Holding left/right on the timeline accelerates from 10-second steps to 30 seconds after 1.2 seconds and 60 seconds after 3 seconds. Releasing, changing direction, or leaving the timeline resets the step to 10 seconds. Button navigation does not accelerate.

Timeline thumbnail previews use authenticated Jellyfin trickplay sprite sheets when available. For libraries without generated trickplay, Aster shows a private snapshot of the decoded video after each seek; hovering over an unplayed position requires server trickplay data. Thumbnail requests are debounced and sheet caches stay bounded. Temporary decoded snapshots are removed when their worker closes.

Horizontal shelves accept horizontal wheel/trackpad input and controller navigation. Vertical input scrolls the page; focus reveals clipped cards without recentering every row.

Chapter boundaries appear as small ticks on the playback timeline. Clicking a tick seeks to its start; focusing or hovering over the timeline shows the chapter title in the player heading. Embedded chapters take priority, with Jellyfin chapter metadata filling in when the file has none.

Press Y during playback (C on a keyboard) or activate the chapter-list button to open Chapters. Use Up/Down and A to jump; playback retains its paused/playing state. X / Start closes the menu, while B stops playback and returns as usual. Titles with no chapter metadata show a brief notice instead of an empty picker.

Rendering sleeps when views are idle and wakes on input, asynchronous results and video frames. See [PERFORMANCE-AUDIT.md](PERFORMANCE-AUDIT.md) for the whole-app media-center audit and reproducible benchmark instructions. Hardware decoding needs the GPU vendor's video driver in addition to Mesa; Intel UHD 630 uses `intel-media-driver` and `intel-gmmlib`.

## Automated builds and downloads

[GitHub Actions](https://github.com/ywnzo/aster/actions/workflows/linux.yml) runs formatting, strict Clippy, Rust tests and embedded OpenGL playback checks on every main-branch push and pull request. Tests use synthetic media under Xvfb/Mesa, never a real server. Each successful run includes a Linux x86_64 archive and SHA256 checksum as an Actions artifact.

Push a tag matching Cargo.toml (`v` followed by the exact package version) to publish a [GitHub Release](https://github.com/ywnzo/aster/releases) after all checks pass. Versions containing a hyphen become prereleases. Release downloads are built on Ubuntu 24.04 and require glibc 2.39+, `libmpv.so.2`, libudev and OpenGL; install the appropriate vendor video driver for hardware decoding. These archives do not bundle a GPU driver or libmpv. The workflow can also be run manually for a build without publishing a release.

After extracting a download, verify it using `sha256sum --check aster-*.tar.gz.sha256` from the directory containing the archive and checksum, then run `scripts/install-local` from the extracted directory. The desktop launcher uses the transparent `aster-star` icon name to avoid older KDE icon-cache entries.
