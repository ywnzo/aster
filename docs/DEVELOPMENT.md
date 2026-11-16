# Development

Aster is a native Rust application. It uses egui/eframe for its interface, gilrs for gamepads, and the libmpv OpenGL render API for in-window playback. It does not run a web development server.

## Build

Rust 1.88+ and native development libraries are required. On Arch Linux:

```sh
sudo pacman -S --needed rust base-devel pkgconf systemd-libs mpv libsecret
cargo build --locked --release
./target/release/aster --windowed
```

On Debian/Ubuntu, install a current Rust toolchain, `pkg-config`, `libudev-dev`, `libmpv-dev`, and `libxkbcommon-x11-0`. Other supported development paths are `scripts/kiri-build` (explicitly targets Kiri's Docker socket) and the [Flatpak build](FLATPAK.md).

## Verify

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
python3 tests/embedded_ui.py "$PWD/target/release/aster" /tmp/aster-ui-check
```

The graphical test needs ffmpeg and a working desktop. It creates synthetic video, subtitles, artwork, and a loopback Jellyfin fixture. It never accesses a real library. Set `ASTER_TEST_TRICKPLAY=1 ASTER_TEST_SERVER_CHAPTERS=1` to test server-provided chapter metadata and preview sheets. Headless CI uses `scripts/ci-ui` with Xvfb, Openbox, and a virtual PulseAudio sink.

## Package

`scripts/package target/release/aster` creates a native archive and SHA256 checksum under `dist/`. `scripts/install-local target/release/aster` installs for the current user. Set `ASTER_INSTALL_PREFIX` to test installation in a separate directory.

GitHub Actions validates main-branch pushes and pull requests. To release, update Cargo.toml/Cargo.lock and RELEASE.md, then push a matching `vVERSION` tag. Hyphenated versions are published as prereleases. See `.github/workflows/` for the exact build and release gates.

## Settings

Native preferences live at `~/.config/aster/settings.json` and optional private-file credentials at `~/.local/share/aster/session.json`, following XDG overrides. Flatpak uses the corresponding directories under `~/.var/app/io.github.ywnzo.Aster/`. Existing Hearth settings are read when no Aster profile exists. Do not commit profiles, tokens, cached artwork, or real-library captures.

## UI inspection

`ASTER_DEV_BRIDGE=/private/directory/control.sock` enables the optional inspection socket. It supports status, captures, and synthetic controller actions. The directory is `0700` and the socket `0600`. `ASTER_DEV_RESTORE` can transfer a session from another running development instance in memory. Both are disabled in ordinary launches.

`ASTER_PREVIEW_DATA` selects synthetic artwork/layout fixtures. Developer actions intentionally cannot start real playback or toggle real watched/favorite state. Keep those guards intact. Store local fixtures and captures outside the repository.
