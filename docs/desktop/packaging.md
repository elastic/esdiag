# Desktop Packaging

This project supports desktop packaging for:

- macOS (`.dmg`)
- Windows (`.msi`, minimum Windows version: 10)
- Linux (Flatpak)

## Scope

- Windows installer format is `.msi` only.
- Flatpak base app version is pinned to `0.15.0`.
- Flatpak runtime/sdk target is `org.gnome.*//49` (moved from GNOME 47 because GNOME 47 reached end-of-life on October 15, 2025).
- Flatpak work in this change is local artifact generation and validation only.
- Publishing/release distribution (GitHub Releases, FlatHub, other remotes) is intentionally out of scope.

## Configuration

- `tauri.conf.json` controls desktop bundles, including Windows `.msi`.
- `packaging/desktop-targets.json` is the source of truth for:
  - Windows minimum version (`10`)
  - Windows bundle format (`msi`)
  - Flatpak base app version (`0.15.0`)
  - Flatpak local-only mode
- `packaging/flatpak/com.elastic.esdiag.json` defines the Flatpak manifest.

## Linux Flatpak Notes

- Build runs inside Flatpak SDK and needs Rust plus `cargo-about` for `build.rs`.
- Notice generation is enabled by default and can be disabled with `ESDIAG_GENERATE_NOTICE=0` for read-only build contexts such as the Docker image build.
- SBOM generation is opt-in via `ESDIAG_GENERATE_SBOM=1` and additionally requires `cargo-sbom`.
- Local workflow requires runtime availability from Flathub:
  - `org.gnome.Platform//49`
  - `org.gnome.Sdk//49`
  - `org.freedesktop.Sdk.Extension.rust-stable//25.08`
- Manifest grants both `x11` and `wayland` sockets and sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` to avoid known compositor/protocol issues observed during local testing.

## Local Build Commands

Validate packaging configuration:

```sh
bash bin/verify-desktop-config.sh
```

Build desktop macOS/Windows bundles with Tauri:

```sh
cargo tauri build --features desktop
```

Build local Flatpak artifact:

```sh
bash bin/build-flatpak-local.sh
```

Generate an SBOM during a build:

```sh
ESDIAG_GENERATE_SBOM=1 cargo build
```

Skip NOTICE generation in read-only build contexts:

```sh
ESDIAG_GENERATE_NOTICE=0 cargo build
```

## Artifact Validation

Validate required artifacts in a staging directory:

```sh
bash bin/validate-desktop-artifacts.sh target/artifacts
```

Expected artifacts:

- `target/artifacts/macos/ESDiag*.dmg`
- `target/artifacts/windows/ESDiag*.msi`
- `target/artifacts/flatpak/com.elastic.esdiag-0.15.0.flatpak`
