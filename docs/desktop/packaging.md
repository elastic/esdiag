# Desktop Packaging

This project supports desktop packaging for:

- macOS (`.dmg`)
- Windows (`.msi`, minimum Windows version: 10)
- Linux (Flatpak)

## Scope

- Windows installer format is `.msi` only.
- Flatpak base app version is pinned to `0.15.0`.
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

## Artifact Validation

Validate required artifacts in a staging directory:

```sh
bash bin/validate-desktop-artifacts.sh target/artifacts
```

Expected artifacts:

- `target/artifacts/macos/ESDiag*.dmg`
- `target/artifacts/windows/ESDiag*.msi`
- `target/artifacts/flatpak/com.elastic.esdiag-0.15.0.flatpak`
