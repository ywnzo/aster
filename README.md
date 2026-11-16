<p align="center"><img src="assets/aster-mark.svg" width="88" alt="Aster"></p>
<h1 align="center">Aster</h1>
<p align="center">Your Jellyfin library. Built for the sofa.</p>
<p align="center"><a href="https://github.com/ywnzo/aster/releases">Download</a> · <a href="#controls">Controls</a> · <a href="docs/DEVELOPMENT.md">Development</a> · <a href="https://github.com/ywnzo/aster/issues">Report a bug</a></p>

Aster is a native Linux Jellyfin client with a controller-first interface. Browse your movies and shows, pick up where you left off, and watch without leaving the app.

**Aster is in alpha.** Expect rough edges and report problems through [GitHub Issues](https://github.com/ywnzo/aster/issues).

## Features

- A home screen with featured titles, Continue Watching, Next Up, and library collections.
- Movie and series browsing, search, favorites, and inline seasons and episodes.
- In-window playback powered by libmpv, with hardware decoding and resume support.
- Audio and subtitle selection, chapters, seek previews, and next-episode autoplay.
- Gamepad navigation, animated focus, button hints, and an on-screen keyboard.
- Settings for display, playback, controller behavior, and saved sign-in.

## Install

Download the Linux x86_64 package from [Releases](https://github.com/ywnzo/aster/releases). A Jellyfin server is required; Aster does not provide media or a server.

### Flatpak

Download the `.flatpak` bundle, then install and launch it:

```sh
flatpak install --user ./aster-VERSION-linux-x86_64.flatpak
flatpak run io.github.ywnzo.Aster
```

Flatpak installs the shared runtime and includes Aster's playback libraries. The bundle is distributed through GitHub; Aster is not currently listed on Flathub. See [Flatpak packaging](docs/FLATPAK.md) for build instructions and permissions.

### Native archive

Download the `.tar.gz` archive and its `.sha256` file into the same directory. Verify the download, extract it, and run the installer inside:

```sh
sha256sum --check aster-VERSION-linux-x86_64.tar.gz.sha256
tar -xzf aster-VERSION-linux-x86_64.tar.gz
cd aster-VERSION-linux-x86_64
./scripts/install-local
```

The native build requires glibc 2.39 or newer, `libmpv.so.2`, libudev, and working OpenGL drivers. X11 sessions also need libxkbcommon-x11. On Arch Linux, install `mpv` and `libsecret`; on Ubuntu, install `libmpv2`, `libsecret-tools`, and `libxkbcommon-x11-0`. Install your GPU's video driver to enable hardware decoding.

## Connect

Open Aster, enter your Jellyfin server address, and sign in with your username and password or use Quick Connect. Remember sign-in is enabled by default. Aster starts windowed; fullscreen is available in Settings or with F11.

Saved sign-in uses your system keyring. If no keyring is available, Settings offers a private-file option; that stores the token unencrypted with access restricted to your user. Your password is never saved. Flatpak keeps its settings separate from the native installation, so sign in once after switching packages.

## Controls

The defaults below use Xbox button names. A/B can be swapped in Settings.

| Button | Library | Playback |
| --- | --- | --- |
| D-pad / left stick | Move focus | Left/right moves between buttons |
| A | Select | Activate the focused control; pause/resume when controls are hidden |
| B | Back / cancel | Stop and return |
| X / Start | Settings | Show or dismiss controls and menus |
| Y | Search | Chapters |
| LB / RB | Previous / next section | Previous / next episode |
| LT / RT | Filter / sort | Audio / subtitles |

**Seeking:** press Up to focus the timeline, then Left/Right to seek. Holding a direction accelerates seeking. Press Down to return to the playback buttons.

**On-screen keyboard:** A types the selected key, Y inserts a space, X deletes, RT toggles Shift, LT toggles symbols, and Start accepts the text. B cancels.

**Keyboard and mouse:** use arrows, Enter, and Escape to navigate; F3 opens search and F11 toggles fullscreen. During playback, Space or a click on the video pauses/resumes, Tab opens controls, A opens audio, S opens subtitles, C opens chapters, and Page Up/Down switches episodes. Escape stops playback.

## Development

Aster is written in Rust with egui, gilrs, and libmpv. See [development instructions](docs/DEVELOPMENT.md), [Flatpak packaging](docs/FLATPAK.md), and the [performance audit](PERFORMANCE-AUDIT.md).

Pull requests run formatting, Clippy, Rust tests, and embedded-player checks. Version tags publish Linux downloads after the required checks pass.

## License

[MIT](LICENSE). Bundled libraries retain their own licenses. Aster is an independent client, unaffiliated with Jellyfin. Its interface takes inspiration from [Fathom](https://github.com/TraceApps/fathom) and [Switchfin](https://github.com/dragonflylee/switchfin).
