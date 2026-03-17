## Why

Currently, ESDiag operates as a CLI tool and a standalone Web application. To improve user accessibility and provide a native desktop experience without requiring terminal usage or manually managing background server processes, we need to wrap the application in a Tauri desktop app. This allows users to double-click and use ESDiag natively.

## What Changes

- Initialize a Tauri project structure and workspace integration for ESDiag.
- Embed the existing Axum web server into the Tauri application lifecycle so it starts when the app opens and stops when it closes.
- Provide a native desktop window that renders the existing Axum-served web frontend (using Datastar and Askama).
- Refactor the server startup logic slightly so it can be initiated programmatically from the Tauri host while preserving existing CLI/Web standalone modes.
- Integrate the frontend UI to communicate naturally within the Tauri window, ensuring no breakage to the current Web UI.

## Capabilities

### New Capabilities

- `tauri-desktop-app`: A Tauri wrapper capability to launch, host, and interact with the ESDiag web interface as a native cross-platform desktop application.

### Modified Capabilities

- None.

## Impact

- **Code/APIs**: The `main` function for the web server may need its setup logic abstracted into a library function to allow the Tauri runner to start the Axum server on a dynamically assigned or configured port.
- **Dependencies**: Adds Tauri dependencies (`tauri`, `tauri-build`, etc.) to the project. 
- **Systems**: Introduces a new compilation target (desktop binaries) while retaining standard CLI and Web compilation.
