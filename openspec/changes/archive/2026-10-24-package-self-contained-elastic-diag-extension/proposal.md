## Why

The `elastic diag` runtime should install as a first-class Elastic CLI extension without requiring Node.js, Cargo, Homebrew, or an independently managed `esdiag` executable on `PATH`. ESDiag already publishes native release binaries, so extension distribution should select, verify, and install the matching binary as one versioned unit.

## What Changes

- Publish a self-contained Elastic CLI extension registered under the intentional short name `diag`.
- Install the ESDiag Rust binary as `elastic-diag` inside the extension installation directory.
- Select a version-matched native artifact for each supported operating system and architecture.
- Verify artifact integrity before installation and include applicable license and notice files.
- Define install, upgrade, rollback, uninstall, and unsupported-platform behavior.
- Reconcile the ESDiag release artifact matrix with the Elastic CLI platform matrix.
- Keep extension distribution independent from diagnostic Job construction, context resolution, and processing.

## Capabilities

### New Capabilities
- `elastic-cli-extension-distribution`: Defines self-contained native packaging, explicit `diag` registration, artifact selection and verification, lifecycle behavior, and release compatibility for the Elastic CLI extension.

### Modified Capabilities

## Impact

- Affects ESDiag release assets, release automation, extension package metadata, and installation documentation.
- May require an Elastic CLI installer manifest or explicit-name enhancement outside this repository.
- Coordinates with the release artifacts consumed by `elastic/homebrew-tools` without making Homebrew a runtime dependency.
- Does not change the Web UI, diagnostic stages, Job execution, or Elasticsearch/Kibana context semantics.
