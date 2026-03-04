## MODIFIED Requirements

### Requirement: Tauri Wrapper Startup
The application SHALL embed the existing Axum web server inside a native Tauri desktop wrapper without modifying the core behavior of the Axum service. The background server must bind to an available port and initialize web runtime mode for desktop-hosted execution.

#### Scenario: User launches the desktop application
- **GIVEN** the Tauri desktop application is built and compiled
- **WHEN** the user opens the application executable
- **THEN** the embedded Axum web server initializes in the background and a Tauri webview window opens, pointing to the local bound port of the Axum server
- **AND** the desktop-hosted web server starts in an explicit runtime mode value used by web handlers

### Requirement: Independent Lifecycles
The desktop application SHALL maintain parity with CLI and Web application code. Starting the application via traditional CLI or Web mechanisms SHALL NOT launch the Tauri interface or cause dependency errors, and runtime mode handling for web execution MUST NOT alter standalone CLI behavior.

#### Scenario: Running as a traditional CLI utility
- **GIVEN** the compiled ESDiag binary
- **WHEN** the user executes the binary from a terminal with specific CLI arguments (e.g., `--help` or data processing commands)
- **THEN** the application operates purely as a CLI tool without initializing the Tauri window framework
- **AND** runtime mode logic used by web interfaces does not change CLI command semantics
