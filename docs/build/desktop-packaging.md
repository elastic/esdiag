# Desktop Packaging

This project supports desktop packaging for:

- macOS (`.dmg`)
- Windows (`.msi`, minimum Windows version: 10)
- Linux (Flatpak)

## Scope

- Windows installer format is `.msi`.
- Flatpak base app version is pinned to `0.15.0`.
- Flatpak runtime/sdk target is `org.gnome.*//49` (moved from GNOME 47 because GNOME 47 reached end-of-life on October 15, 2025).
- Flatpak work in this change is local artifact generation and validation only.
- Publishing/release distribution (GitHub Releases, FlatHub, other remotes) is intentionally out of scope.

## Configuration

- Root `tauri.conf.json` is the Tauri CLI and `tauri-build` source of truth for repo-root desktop builds.
- `desktop/tauri.conf.json` remains as a desktop-scoped mirror with paths relative to `desktop/`.
- `desktop/packaging/desktop-targets.json` is the source of truth for:
  - Windows minimum version (`10`)
  - Windows bundle format (`msi`)
  - Flatpak base app version (`0.15.0`)
  - Flatpak local-only mode
- `desktop/packaging/flatpak/com.elastic.esdiag.json` defines the Flatpak manifest.

## Linux Flatpak Notes

- Flatpak builds run inside the Flatpak SDK and disable NOTICE generation in the manifest, so they do not require `cargo-about`.
- Notice generation is enabled by default elsewhere and can be disabled with `ESDIAG_GENERATE_NOTICE=0` for read-only build contexts such as the Docker image build.
- SBOM generation is opt-in via `ESDIAG_GENERATE_SBOM=1` and additionally requires `cargo-sbom`.
- Local workflow requires runtime availability from Flathub:
  - `org.gnome.Platform//49`
  - `org.gnome.Sdk//49`
  - `org.freedesktop.Sdk.Extension.rust-stable//25.08`
- Manifest grants both `x11` and `wayland` sockets and sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` to avoid known compositor/protocol issues observed during local testing.

## Local Build Commands

Validate packaging configuration:

```sh
bash desktop/scripts/verify-desktop-config.sh
```

Normalize the package version to an MSI-safe form:

```sh
bash desktop/scripts/normalize-cargo-version-for-msi.sh Cargo.toml
```

Run the local regression test for MSI version normalization:

```sh
bash desktop/scripts/test-normalize-cargo-version-for-msi.sh
```

Build desktop macOS/Windows bundles with Tauri:

```sh
cargo tauri build --features desktop
```

Build a local Windows raw app artifact with Docker Buildx:

```sh
bash desktop/scripts/buildx-windows.sh
```

At the moment, the local Buildx path is experimental.

- Windows Buildx output is only the raw desktop app layout:
  - `esdiag.exe`
  - `WebView2Loader.dll`
- Windows Buildx does not produce an `.msi`.
- The official Windows `.msi` bundle is produced by the native `windows-latest` GitHub Actions job.
- That MSI CI path sets `ESDIAG_GENERATE_NOTICE=0`, so it does not require `cargo-about`.

Build local Flatpak artifact:

```sh
bash desktop/scripts/build-flatpak-local.sh
```

## CI Workflow

The `Desktop Artifacts` GitHub Actions workflow is intentionally manual.

- PR updates do not trigger desktop artifact builds automatically.
- When a branch is ready for packaging validation, run the workflow with `workflow_dispatch` from the Actions tab and select the PR branch/ref you want to build.

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
bash desktop/scripts/validate-desktop-artifacts.sh target/artifacts
```

Expected CI artifacts:

- `target/artifacts/macos/*.dmg`
- `target/artifacts/windows/*.msi`
- `target/artifacts/flatpak/com.elastic.esdiag-0.15.0.flatpak`
