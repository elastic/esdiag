## 1. Project Setup

- [x] 1.1 Add `tauri` and `tauri-build` to the project (either as workspace members or to the root `Cargo.toml`).
- [x] 1.2 Initialize the `src-tauri` directory structure with default Tauri configurations (`tauri.conf.json`, `build.rs`).
- [x] 1.3 Ensure `tauri.conf.json` is configured to build and package an application without an external frontend bundler (using `beforeBuildCommand` and `beforeDevCommand` as no-ops or appropriate empty hooks).

## 2. Server Refactoring

- [x] 2.1 Extract the core Axum router initialization and server listening logic into a reusable function in the main codebase (e.g., `start_server(port: Option<u16>) -> Result<SocketAddr>`).
- [x] 2.2 Update the existing standalone Web CLI command to call the extracted function, maintaining current behavior.
- [x] 2.3 Ensure the server can bind to `127.0.0.1:0` to dynamically acquire an available port when embedded.

## 3. Tauri Integration

- [x] 3.1 Implement the Tauri `setup` hook in `src-tauri/src/main.rs` (or `src-tauri/src/lib.rs`) to spawn the Axum server on a background `tokio` thread.
- [x] 3.2 Within the Tauri `setup` hook, await the bound port of the embedded Axum server and dynamically construct or update the Tauri main window to navigate to `http://localhost:{port}`.
- [x] 3.3 Wire up the main entrypoint: If the user launches the binary without CLI arguments, initialize the Tauri app; if CLI arguments are provided, bypass Tauri and execute the CLI/Web commands as before.

## 4. Verification & Polish

- [x] 4.1 Verify the standalone CLI functionality operates without regressions (e.g., executing commands doesn't pop up a Tauri window).
- [x] 4.2 Verify the standalone Web server functionality operates without regressions.
- [x] 4.3 Build and launch the Tauri desktop application to confirm the UI is correctly rendered inside the native window.
## 5. Feature Flags (Post-Requirement Fix)

- [x] 5.1 Define a new feature flag `desktop` in `Cargo.toml`.
- [x] 5.2 Make `tauri` and `tauri-build` optional dependencies tied to the `desktop` feature.
- [x] 5.3 Ensure the `desktop` feature is NOT included in the `default` features.
- [x] 5.4 Update `build.rs` to only run `tauri_build::build()` when `feature = "desktop"` is active.
- [x] 5.5 Update `src/main.rs` to guard the Tauri entrypoint initialization with `#[cfg(feature = "desktop")]`. Ensure the code falls back or exits with an error if no arguments are passed and `desktop` is not enabled.
- [x] 5.6 Run `cargo check` and verify the project builds properly both with and without the `desktop` feature flag.
