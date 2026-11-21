# workspace-crate-architecture Specification

## Purpose

Defines how the ESDiag Cargo workspace is divided into crates: the crate layout and dependency direction, how the crates are published alongside `esdiag`, and the packaging, rebuild-scope, and compatibility guarantees maintainers and `cargo install` users rely on.

## Requirements

### Requirement: GUI Crate Layout
The workspace SHALL contain an `esdiag-gui` crate holding the web server, its templates and embedded web and documentation assets, and the Tauri desktop wrapper, and an `esdiag-core` crate holding the remaining ESDiag library code. `esdiag-gui` SHALL depend on `esdiag-core`. `esdiag-core` SHALL NOT depend on `esdiag-gui`. Only the root `esdiag` package SHALL depend on `esdiag-gui`.

#### Scenario: Core does not depend on the GUI
- **GIVEN** the workspace builds successfully
- **WHEN** a maintainer lists the normal dependency tree of `esdiag-core`
- **THEN** the tree does not contain `esdiag-gui`

#### Scenario: Only the binary depends on the GUI
- **GIVEN** any workspace package other than `esdiag-gui` and the root `esdiag` package
- **WHEN** a maintainer lists that package's normal dependency tree
- **THEN** the tree does not contain `esdiag-gui`

#### Scenario: Maintainer tooling avoids the GUI
- **GIVEN** the maintainer tool that generates portable lite collection rules
- **WHEN** a maintainer builds that tool
- **THEN** the build does not compile `esdiag-gui`

### Requirement: Desktop App Wraps The Web Interface
The `desktop` feature SHALL continue to produce a desktop app that starts the embedded web server on a local ephemeral port and displays it in a window, with the startup and shutdown logic provided by `esdiag-gui`. `cargo tauri build --features desktop` from the repository root SHALL continue to produce the desktop bundles.

#### Scenario: Desktop app opens the embedded web interface
- **GIVEN** the `esdiag` binary built with the `desktop` feature
- **WHEN** the user launches it without a subcommand
- **THEN** the embedded server starts on `127.0.0.1` with an ephemeral port
- **AND** the desktop window navigates to that server

#### Scenario: Closing the desktop window stops the server
- **GIVEN** the desktop app is running with its embedded server
- **WHEN** the user closes all windows
- **THEN** the embedded server shuts down

#### Scenario: Desktop bundle build is unchanged
- **GIVEN** a repository checkout with the Tauri CLI installed
- **WHEN** a maintainer runs `cargo tauri build --features desktop` from the repository root
- **THEN** the desktop bundle for the host platform is produced

### Requirement: Lockstep Publication With esdiag
`esdiag-core` and `esdiag-gui` SHALL be published to crates.io with every `esdiag` release, at the same version as `esdiag`. `esdiag` SHALL require each of them at exactly its own version, and `esdiag-gui` SHALL require `esdiag-core` at exactly its own version. Both crates SHALL use the same license as `esdiag` and SHALL state in their package description and README that they are implementation crates of `esdiag` without a supported public API. `elasticrc` SHALL keep its own version and release cadence.

#### Scenario: Versions match
- **GIVEN** the workspace at any commit
- **WHEN** a maintainer reads the manifests of `esdiag`, `esdiag-core`, and `esdiag-gui`
- **THEN** all three declare the same version
- **AND** every dependency between them is an exact requirement for that version

#### Scenario: Release dry run covers all packages
- **GIVEN** a release branch with a release version set
- **WHEN** a maintainer runs a multi-package publish dry run for `esdiag`, `esdiag-core`, and `esdiag-gui`
- **THEN** every package is packaged and verified in dependency order
- **AND** the dry run does not include `elasticrc` or `repo-tools`

#### Scenario: Install from crates.io after publication
- **GIVEN** `esdiag`, `esdiag-core`, and `esdiag-gui` are published at the same version
- **WHEN** a user runs `cargo install esdiag` for that version
- **THEN** the binary builds from the published packages alone

### Requirement: Published Packages Are Self-Contained
Every published package SHALL contain all files it embeds or includes at compile time. A crate built from its published package SHALL embed the same content as the same crate built in a repository checkout. Canonical file locations in the repository SHALL remain the single source that maintainers edit.

#### Scenario: Packaged crate builds without the repository
- **GIVEN** the packaged form of `esdiag-core` or `esdiag-gui`
- **WHEN** Cargo verifies the package by building it outside the workspace
- **THEN** the build succeeds

#### Scenario: Packaged files match the repository
- **GIVEN** a release branch after asset synchronization
- **WHEN** a maintainer compares the embedded files inside each package with their canonical repository files
- **THEN** they are identical

#### Scenario: Unsynchronized assets block packaging
- **GIVEN** a canonical embedded file that has changed since the last synchronization
- **WHEN** a maintainer runs the release packaging check
- **THEN** the check fails and names the out-of-date file

### Requirement: Rebuild Scope Follows The Layout
An edit confined to `esdiag-gui` SHALL NOT cause Cargo to recompile `esdiag-core`.

#### Scenario: Server edit leaves core cached
- **GIVEN** a completed debug build of the workspace
- **WHEN** a maintainer modifies a source file in `esdiag-gui` and runs a debug build
- **THEN** Cargo recompiles `esdiag-gui` and the root package
- **AND** Cargo does not recompile `esdiag-core`

### Requirement: User-Visible Behavior Is Preserved
Splitting the workspace SHALL NOT change any user-visible behavior of the CLI, Web UI, desktop app, setup assets, or processed diagnostic output for Elasticsearch, Kibana, Logstash, or ECK. The released binary name SHALL remain `esdiag`.

#### Scenario: CLI surface is unchanged
- **GIVEN** the `esdiag` binary built before and after the split with default features
- **WHEN** a maintainer compares the help output of every command and subcommand
- **THEN** the outputs are identical

#### Scenario: Archive processing output is unchanged
- **GIVEN** the diagnostic archive fixtures in the repository test suite
- **WHEN** each fixture is processed to a directory output before and after the split
- **THEN** the produced documents are identical apart from run-specific identifiers and timestamps

#### Scenario: Existing tests pass
- **GIVEN** the workspace after the split
- **WHEN** a maintainer runs the full workspace test suite with default features
- **THEN** all tests that passed before the split pass

### Requirement: Cargo Features Are Preserved
The root `esdiag` package SHALL keep the `default`, `full`, `agent`, `server`, `setup`, `keystore`, `desktop`, and `elasticrc` features with their current meaning, and each feature SHALL gate the same functionality as before the split. `esdiag-gui` SHALL be compiled only when the `server` feature is enabled, and Tauri SHALL be compiled only when the `desktop` feature is enabled.

#### Scenario: Minimal build excludes the GUI
- **GIVEN** the root package
- **WHEN** a maintainer builds it with default features disabled
- **THEN** the build succeeds
- **AND** neither `esdiag-gui` nor Tauri is compiled

#### Scenario: Server build excludes Tauri
- **GIVEN** the root package
- **WHEN** a maintainer builds it with default features disabled and only the `server` feature enabled
- **THEN** `esdiag-gui` is compiled
- **AND** Tauri is not compiled

#### Scenario: Each feature builds alone
- **GIVEN** the root package
- **WHEN** a maintainer builds it with default features disabled and exactly one of `agent`, `server`, `setup`, `keystore`, or `elasticrc` enabled
- **THEN** each build succeeds
