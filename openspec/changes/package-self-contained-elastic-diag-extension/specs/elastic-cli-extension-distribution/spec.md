## Purpose

Define a verifiable, self-contained native distribution contract for installing ESDiag under the intentional Elastic CLI command name `elastic diag`.

## ADDED Requirements

### Requirement: Explicit Extension Identity
The published extension SHALL register the command name `diag` independently of the `elastic/esdiag` repository name and SHALL install its executable entrypoint as `elastic-diag` on Unix-like systems or `elastic-diag.exe` on Windows.

#### Scenario: Installed command uses intentional short name
- **WHEN** a user installs the published extension
- **THEN** Elastic CLI registers the command as `elastic diag`
- **AND** it does not expose the extension as `elastic esdiag`

### Requirement: Self-Contained Native Runtime
The extension SHALL install the ESDiag native Rust executable inside the extension installation directory. Runtime execution SHALL NOT require Node.js, npm, Cargo, Homebrew, or a separately installed `esdiag` executable on `PATH`.

#### Scenario: Execute without external ESDiag installation
- **GIVEN** no `esdiag` executable is available on `PATH`
- **AND** Node.js, Cargo, and Homebrew are unavailable
- **WHEN** the user runs `elastic diag`
- **THEN** Elastic CLI executes the extension-owned native binary

#### Scenario: PATH executable cannot shadow packaged runtime
- **GIVEN** a different `esdiag` version is available on `PATH`
- **WHEN** the user runs `elastic diag`
- **THEN** the extension executes its version-matched packaged binary
- **AND** the PATH executable is ignored

### Requirement: Platform-Specific Artifact Selection
Extension metadata SHALL map supported Elastic CLI operating-system and architecture combinations to versioned ESDiag release artifacts. The initial publication target SHALL include macOS ARM64, macOS x86-64, Linux ARM64, Linux x86-64, and Windows x86-64.

#### Scenario: Supported platform selects one artifact
- **WHEN** installation runs on a declared supported platform
- **THEN** the installer selects exactly one artifact matching the current operating system, architecture, and extension version

#### Scenario: Unsupported platform fails before installation
- **WHEN** no artifact is declared for the current operating system and architecture
- **THEN** installation fails with a message listing supported platforms
- **AND** the installer does not build from source or select an artifact for another platform

### Requirement: Artifact Integrity and Contents
Every declared native artifact SHALL have a SHA-256 checksum in trusted extension metadata. Installation SHALL verify the downloaded archive before extraction. Each archive SHALL contain the executable, `LICENSE.txt`, and `NOTICE.txt`.

#### Scenario: Valid artifact installs
- **GIVEN** the downloaded artifact matches its declared SHA-256 checksum
- **AND** all required archive files are present
- **WHEN** installation proceeds
- **THEN** the executable and legal notices are installed within the extension directory

#### Scenario: Checksum mismatch fails closed
- **WHEN** a downloaded artifact does not match its declared SHA-256 checksum
- **THEN** installation fails before executing or installing archive contents
- **AND** the previous working extension version remains available

#### Scenario: Incomplete archive is rejected
- **WHEN** an artifact omits the executable, `LICENSE.txt`, or `NOTICE.txt`
- **THEN** installation fails with an error naming the missing entry

### Requirement: Version Compatibility Contract
The extension package version, selected artifact version, and version reported by `elastic-diag` SHALL match. Metadata SHALL declare the minimum compatible Elastic CLI version.

#### Scenario: Installed versions agree
- **WHEN** installation completes
- **THEN** `elastic-diag version` reports the extension package version
- **AND** the selected artifact name and metadata identify that same version

#### Scenario: Elastic CLI is too old
- **WHEN** the installed Elastic CLI version is below the declared minimum
- **THEN** installation fails with the required minimum version
- **AND** no incompatible extension is activated

### Requirement: Atomic Extension Lifecycle
Install and upgrade operations SHALL stage and validate a new extension version before activation. Failed installation or upgrade SHALL leave the previously active version usable. Uninstall SHALL remove only extension-owned files.

#### Scenario: Successful upgrade activates new version
- **GIVEN** an older extension version is installed
- **WHEN** a newer version is downloaded, verified, and passes its smoke check
- **THEN** Elastic CLI atomically activates the newer version
- **AND** subsequent `elastic diag` invocations use it

#### Scenario: Failed upgrade preserves previous version
- **GIVEN** a working extension version is installed
- **WHEN** validation of an upgrade fails
- **THEN** the working version remains active
- **AND** partially staged files are not used for execution

#### Scenario: Uninstall preserves independent installations
- **GIVEN** ESDiag is also installed through Homebrew, Cargo, or another package manager
- **WHEN** the user uninstalls the Elastic CLI extension
- **THEN** only files owned by the extension installation are removed
- **AND** independent ESDiag installations are unchanged

### Requirement: Release Publication Gate
An extension version SHALL NOT be published until every declared artifact is available, checksum-verified, contains the required legal files, reports the expected version, and passes an extension invocation smoke test.

#### Scenario: Incomplete release is not published
- **GIVEN** one declared platform artifact is missing or fails validation
- **WHEN** publication automation evaluates the release
- **THEN** extension publication is blocked
- **AND** no partial platform manifest is released under that version
