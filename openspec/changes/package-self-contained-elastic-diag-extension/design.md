## Context

The extension runtime and context behavior are defined by `register-elastic-cli-extension`. This change covers distribution only.

ESDiag release archives currently provide native binaries for macOS ARM64, Linux ARM64, and Linux x86-64, together with a checksum file, `LICENSE.txt`, and `NOTICE.txt`. The Homebrew formula consumes those binaries but builds macOS x86-64 from source. There is no current Windows x86-64 release artifact. The experimental Elastic CLI installer derives extension names from package or repository names and does not yet provide the explicit-name, platform-artifact manifest needed to install `elastic/esdiag` as `diag`.

## Goals / Non-Goals

**Goals:**

- Define one authoritative extension manifest for identity, compatibility, artifacts, and checksums.
- Reuse ESDiag GitHub release binaries without adding a runtime package-manager dependency.
- Close the macOS x86-64 and Windows x86-64 release gaps.
- Make install and upgrade fail closed and preserve the previous working version.
- Keep extension and binary versions synchronized.

**Non-Goals:**

- Do not change ESDiag command parsing, context resolution, Jobs, or diagnostic stages.
- Do not make Homebrew responsible for Elastic CLI extension installation.
- Do not compile Rust during extension installation.
- Do not use a PATH lookup as a fallback.
- Do not define a second ESDiag release version for the extension.

## Decisions

### Add a native extension manifest contract to Elastic CLI

The preferred installation contract is a declarative manifest consumed directly by Elastic CLI. The manifest contains:

- a schema version;
- explicit extension name `diag`;
- extension and ESDiag version;
- minimum Elastic CLI version;
- one download URL and SHA-256 checksum per supported platform;
- the installed entrypoint name; and
- required legal-file names.

This requires an upstream Elastic CLI installer enhancement because repository-name derivation cannot produce `diag` from `elastic/esdiag` and the current installer does not select native release artifacts.

Alternative considered: publish an npm package named `elastic-diag` with platform-specific optional dependencies. Rejected as the primary design because it makes Node/npm part of installation, duplicates platform metadata, and is a poor fit for users of the native Elastic CLI. It remains a contingency only if the native installer cannot accept a manifest.

### Keep the manifest generated from an ESDiag release

Release automation generates the extension manifest only after the ESDiag tag, artifacts, and checksum file exist. The ESDiag version is the extension version; there is no independently drifting wrapper version.

The generated manifest is published from an installer-supported location associated with the ESDiag release. Elastic CLI copies the selected artifact into its own extension directory, extracts the legal files, and installs or renames `esdiag` to `elastic-diag` (`elastic-diag.exe` on Windows). Renaming is intentional because the runtime uses its executable name to select the extension profile.

Alternative considered: check in a mutable manifest that always points to `latest`. Rejected because upgrades would not be reproducible and checksum/version mismatches would be harder to detect.

### Reuse the release archive format

Native archives continue to contain a root-level ESDiag executable, `LICENSE.txt`, and `NOTICE.txt`. The existing checksum list remains useful to Homebrew and release verification; extension metadata copies the exact checksum for each selected archive so installation does not trust an unverified archive or infer a checksum.

The extension does not repackage platform binaries merely to rename the executable. This keeps Homebrew and Elastic CLI on the same compiled artifact and reduces release duplication.

Alternative considered: publish separate `elastic-diag` archives. Rejected because byte-identical binaries would acquire two release identities and independent checksums.

### Complete the native platform matrix before first publication

The initial extension matrix is:

- `aarch64-apple-darwin`;
- `x86_64-apple-darwin`;
- `aarch64-unknown-linux-gnu`;
- `x86_64-unknown-linux-gnu`; and
- `x86_64-pc-windows-msvc`.

Release automation must add macOS x86-64 and Windows x86-64 binaries. Homebrew may continue its source fallback temporarily, but the extension never does. Additional targets require an explicit manifest entry and the same validation gates.

### Make installation transactional

Elastic CLI downloads into a staging directory, verifies SHA-256, validates required entries, extracts without permitting path traversal, runs `elastic-diag version`, and only then activates the directory. Upgrade retains the active version until all checks pass. Uninstall deletes only the selected extension version and extension-owned metadata.

The installer, rather than ESDiag runtime code, owns download, extraction, activation, rollback, and uninstall state.

### Gate publication with end-to-end artifact checks

Publication automation validates every manifest entry by downloading the archive, checking its SHA-256, inspecting archive paths, verifying legal files and executable permissions where applicable, and invoking the extracted executable through its final `elastic-diag` name.

The smoke test verifies the reported version and extension-profile help. Publication is all-or-nothing for the declared initial matrix.

## Risks / Trade-offs

- Upstream Elastic CLI manifest support is required → Land and release the installer contract before publishing the ESDiag extension manifest.
- Cross-platform release jobs can delay an otherwise healthy ESDiag release → Generate extension metadata only after the full declared matrix succeeds; ordinary ESDiag releases remain independently consumable.
- SHA-256 proves integrity against manifest metadata but is not artifact signing → Serve manifest and artifacts through authenticated GitHub/Elastic release infrastructure and leave signing as a compatible future enhancement.
- Renaming the executable is required for profile selection → Include final-name invocation in every platform smoke test.
- Windows archive and executable conventions differ → Use a platform-aware archive entry and final `.exe` name in manifest validation.

## Migration Plan

1. Land the native `elastic-diag` runtime profile from `register-elastic-cli-extension`.
2. Extend ESDiag release automation to produce and validate the full native platform matrix.
3. Add explicit extension identity and native artifact manifest support to Elastic CLI.
4. Generate a candidate manifest from one stable ESDiag release and run install, upgrade, rollback, and uninstall tests on every declared platform.
5. Publish the manifest only after all gates pass.
6. Update extension documentation to replace local wrapper registration with the self-contained install command.

Rollback removes or disables the published manifest version in the extension catalog. Existing installations retain their last verified files until explicitly uninstalled or upgraded.
