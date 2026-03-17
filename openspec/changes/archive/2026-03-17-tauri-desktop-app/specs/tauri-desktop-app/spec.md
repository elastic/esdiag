## ADDED Requirements

### Requirement: Tauri Wrapper Startup
The application SHALL embed the existing Axum web server inside a native Tauri desktop wrapper without modifying the core behavior of the Axum service. The background server must bind to an available port.

#### Scenario: User launches the desktop application
- **GIVEN** the Tauri desktop application is built and compiled
- **WHEN** the user opens the application executable
- **THEN** the embedded Axum web server initializes in the background and a Tauri webview window opens, pointing to the local bound port of the Axum server

### Requirement: Port Selection
The Tauri wrapper SHALL dynamically assign an available port or use a specific fallback for the embedded Axum server to avoid port collision with other local services.

#### Scenario: Tauri desktop initialization avoids port conflicts
- **GIVEN** an active environment where port 3000 may be in use
- **WHEN** the Tauri application spawns the web server task
- **THEN** the Axum server binds successfully to an open port and the Tauri main window correctly navigates to the assigned URL

### Requirement: Independent Lifecycles
The desktop application SHALL maintain parity with CLI and Web application code. Starting the application via traditional CLI or Web mechanisms SHALL NOT launch the Tauri interface or cause dependency errors.

#### Scenario: Running as a traditional CLI utility
- **GIVEN** the compiled ESDiag binary
- **WHEN** the user executes the binary from a terminal with specific CLI arguments (e.g., `--help` or data processing commands)
- **THEN** the application operates purely as a CLI tool without initializing the Tauri window framework

### Requirement: Optional Compilation (Feature Flag)
The Tauri wrapper and all its dependencies SHALL be completely optional and guarded by a cargo feature flag (e.g., `desktop` or `tauri`). It MUST NOT be included in the `default` features of the workspace.

#### Scenario: Building the default CLI or Web application
- **GIVEN** the `esdiag` cargo workspace
- **WHEN** a user compiles the application using `cargo build`
- **THEN** the Tauri wrapper code and `tauri` dependencies are excluded from the build, resulting in a standard standalone CLI/Web binary

#### Scenario: Building the desktop application
- **GIVEN** the `esdiag` cargo workspace
- **WHEN** a user compiles the application using `cargo build --features desktop` (or equivalent feature name)
- **THEN** the Tauri wrapper code is compiled and the desktop application features are included in the resulting binary
