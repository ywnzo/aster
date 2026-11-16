# Flatpak

Aster's Flatpak ID is `io.github.ywnzo.Aster`. The source manifest is in `packaging/flatpak/`. It builds Aster and its playback libraries from pinned sources against Freedesktop 26.08, with Cargo dependencies fetched by checksum before an offline Rust build.

## Install a release

Install Flatpak 1.16 or newer, download the `.flatpak` file from [GitHub Releases](https://github.com/ywnzo/aster/releases), and run:

```sh
flatpak install --user ./aster-VERSION-linux-x86_64.flatpak
flatpak run io.github.ywnzo.Aster
```

The bundle declares Flathub as its runtime source, so Flatpak can install the shared Freedesktop runtime. Aster itself is distributed through GitHub, not listed on Flathub. Install a newer bundle with the same command to update it. A single-file bundle does not provide automatic app updates from a repository.

## Build locally

Install `flatpak`, `flatpak-builder`, and Python 3.11+. From the project root:

```sh
./scripts/flatpak-cargo-sources --check
./scripts/flatpak-build
```

The first build downloads the runtime, SDK, Rust SDK extension, and pinned dependencies. It takes more disk space than installing a release. Build state, the local OSTree repository, the bundle, and its checksum are under `dist/`. The build script installs missing SDK/runtime dependencies for your user; it does not replace your native Aster installation.

When Cargo.lock changes, run `scripts/flatpak-cargo-sources` and commit the updated `packaging/flatpak/cargo-sources.json`. Registry archives use the checksums already recorded in Cargo.lock. Non-registry dependencies must be pinned explicitly.

## Sandbox access

- Network: connect to Jellyfin and load artwork/streams.
- Wayland or fallback X11, plus shared IPC: display the app.
- PulseAudio: audio output through PulseAudio or PipeWire's PulseAudio service.
- DRI: GPU rendering and hardware video decoding.
- Input devices and read-only `/run/udev`: detect and use controllers, including reconnects.
- Bigscreen input-service D-Bus: detect its presence to filter mirrored controller key events. Aster does not change the service.
- Secret Service D-Bus: save sign-in to the desktop keyring through the bundled `secret-tool`.

The package does not request access to your home directory or the whole host filesystem. It stores its own settings/data under `~/.var/app/io.github.ywnzo.Aster/`. Native settings and sign-in are not automatically imported; sign in once in the Flatpak. Choose private-file storage in Settings only if you prefer it to the keyring.

## Verification and releases

GitHub Actions builds the bundle, then installs it in a separate Ubuntu job. That job exercises the actual sandboxed application with synthetic media, virtual audio and Xvfb/Openbox. The fixture grants access only to its temporary directory; normal launches use the manifest permissions above. Physical controller reconnection and hardware decoding also need testing on the target hardware.

Tagged releases wait for the native checks and Flatpak checks before attaching the archives, bundle and checksums. GPU drivers are supplied by Flatpak's matching graphics extensions and the host kernel; host-specific private libmpv/driver directories are not copied into the bundle.

To exercise a built bundle on an existing desktop, run `ASTER_TEST_DESKTOP=host scripts/ci-flatpak-ui /absolute/path/to/aster.flatpak /tmp/aster-flatpak-ui`. This installs the package for your user and runs only the synthetic fixture.
