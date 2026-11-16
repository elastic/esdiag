## 1. Native Extension Manifest Contract

- [ ] 1.1 Define the versioned manifest schema for explicit `diag` identity, minimum Elastic CLI version, final entrypoint name, platform artifacts, SHA-256 checksums, and required legal files.
- [ ] 1.2 Add manifest fixtures and schema validation tests for supported, unsupported, malformed, duplicate, and incomplete platform entries.
- [ ] 1.3 Implement or coordinate the Elastic CLI installer enhancement that consumes explicit extension names and native platform artifact maps.
- [ ] 1.4 Add an installer contract test proving `elastic/esdiag` metadata registers `elastic diag` instead of `elastic esdiag`.

## 2. ESDiag Native Release Matrix

- [ ] 2.1 Add a release artifact for `x86_64-apple-darwin`.
- [ ] 2.2 Add a release artifact for `x86_64-pc-windows-msvc`.
- [ ] 2.3 Ensure every native archive contains the executable, `LICENSE.txt`, and `NOTICE.txt` at validated paths.
- [ ] 2.4 Extend checksum generation and artifact verification to all five initial platform targets.
- [ ] 2.5 Verify each extracted binary reports the release version when invoked under its final `elastic-diag` or `elastic-diag.exe` name.
- [ ] 2.6 Update `elastic/homebrew-tools` automation to consume the macOS x86-64 binary instead of building that platform from source.

## 3. Self-Contained Installation

- [ ] 3.1 Generate an immutable extension manifest from a completed ESDiag release and its verified checksum file.
- [ ] 3.2 Implement platform selection that chooses exactly one operating-system and architecture match and rejects unsupported platforms.
- [ ] 3.3 Stage downloads in an extension-owned temporary directory and verify SHA-256 before extraction.
- [ ] 3.4 Reject archive path traversal, missing legal files, missing executables, and invalid executable names or permissions.
- [ ] 3.5 Install or rename the release executable to `elastic-diag` on Unix-like systems and `elastic-diag.exe` on Windows.
- [ ] 3.6 Verify extension execution succeeds without Node.js, Cargo, Homebrew, or `esdiag` on `PATH`.
- [ ] 3.7 Verify an unrelated PATH-installed `esdiag` cannot shadow the packaged runtime.

## 4. Lifecycle and Publication

- [ ] 4.1 Implement staged activation so a new install or upgrade becomes active only after checksum, content, version, and smoke validation pass.
- [ ] 4.2 Add rollback tests proving a failed upgrade leaves the previous extension version usable.
- [ ] 4.3 Add uninstall tests proving only extension-owned files are removed.
- [ ] 4.4 Gate publication on successful validation of every declared platform artifact.
- [ ] 4.5 Publish one candidate extension version and test clean install, upgrade, failed-upgrade rollback, and uninstall on every declared platform.

## 5. Documentation and Verification

- [ ] 5.1 Document the self-contained install and upgrade commands, supported platforms, version contract, and unsupported-platform errors.
- [ ] 5.2 Remove PATH, Node.js, Cargo, and Homebrew prerequisites from published extension installation guidance.
- [ ] 5.3 Run `cargo test`.
- [ ] 5.4 Run `cargo clippy`.
- [ ] 5.5 Run `openspec verify` after implementation and confirm the distribution change is ready to archive.
