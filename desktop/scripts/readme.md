# Desktop Build Scripts

1. [`verify-desktop-config.sh`](../../docs/build/desktop-packaging.md) - Validate desktop bundle configuration (Windows `.msi`, Flatpak `0.15.0`)
2. [`normalize-cargo-version-for-msi.sh`](../../docs/build/desktop-packaging.md) - Rewrite the package version to an MSI-safe form for Windows packaging
3. [`test-normalize-cargo-version-for-msi.sh`](../../docs/build/desktop-packaging.md) - Local regression test for MSI version normalization
4. [`build-flatpak-local.sh`](../../docs/build/desktop-packaging.md) - Build local Flatpak artifact for Linux
5. [`buildx-windows.sh`](../../docs/build/desktop-packaging.md) - Build local experimental Windows raw app artifacts (`.exe` + `WebView2Loader.dll`) with Docker Buildx
6. [`validate-desktop-artifacts.sh`](../../docs/build/desktop-packaging.md) - Enforce required macOS/Windows/Flatpak artifact outputs
