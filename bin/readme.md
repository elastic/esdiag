# Elastic Stack Diagnostics Additional Executables

1. [`esdiag-control`](../docs/bin/esdiag-control.md) - Setup, build or run Elastic Stack Diagnostics
2. [`min-diag.sh`](../docs/bin/min-diag.md) - Minimal-dependency diagnostic collection for Elasticsearch
3. [`verify-desktop-config.sh`](../docs/desktop/packaging.md) - Validate desktop bundle configuration (Windows `.msi`, Flatpak `0.15.0`)
4. [`normalize-cargo-version-for-msi.sh`](../docs/desktop/packaging.md) - Rewrite the package version to an MSI-safe form for Windows packaging
5. [`test-normalize-cargo-version-for-msi.sh`](../docs/desktop/packaging.md) - Local regression test for MSI version normalization
6. [`build-flatpak-local.sh`](../docs/desktop/packaging.md) - Build local Flatpak artifact for Linux
7. [`buildx-windows.sh`](../docs/desktop/packaging.md) - Build local experimental Windows raw app artifacts (`.exe` + `WebView2Loader.dll`) with Docker Buildx
8. [`validate-desktop-artifacts.sh`](../docs/desktop/packaging.md) - Enforce required macOS/Windows/Flatpak artifact outputs
