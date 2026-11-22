## MODIFIED Requirements

### Requirement: Cargo Features Are Preserved
The root `esdiag` package SHALL keep the `default`, `full`, `agent`, `server`, `setup`, `keystore`, `desktop`, `elasticrc`, and `mcp` features with their current meaning, and each feature SHALL gate the same functionality as before the split. `full` SHALL include `mcp`. `esdiag-gui` SHALL be compiled only when the `server` feature is enabled, and Tauri SHALL be compiled only when the `desktop` feature is enabled.

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
- **WHEN** a maintainer builds it with default features disabled and exactly one of `agent`, `server`, `setup`, `keystore`, `elasticrc`, or `mcp` enabled
- **THEN** each build succeeds

#### Scenario: MCP build excludes the GUI
- **GIVEN** the root package
- **WHEN** a maintainer builds it with default features disabled and only the `mcp` feature enabled
- **THEN** the build succeeds
- **AND** neither `esdiag-gui` nor Tauri is compiled
