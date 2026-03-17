## Context

ESDiag currently functions as either a CLI utility or a web server powered by Axum, Askama, and Datastar. The project is transitioning to optionally provide a native desktop app wrapper using Tauri, without discarding or significantly altering the existing standalone modes. We want to embed the Axum backend such that it runs inside the Tauri application's lifecycle, serving the frontend web view within the native desktop window.

## Goals / Non-Goals

**Goals:**
- Provide a robust Tauri wrapper around the existing ESDiag web server functionality.
- Run the Axum web server in the background as part of the Tauri application lifecycle.
- Automatically connect the Tauri webview to the embedded Axum server.
- Ensure minimum impact to the standalone CLI and Web application code.

**Non-Goals:**
- Re-writing frontend views to native desktop UI toolkits; we will rely entirely on the existing Askama + Datastar setup.
- Inter-Process Communication (IPC) via Tauri commands, as the existing Axum endpoints will handle all API logic (to keep parity between Web and Desktop modes).

## Decisions

- **Axum Integration Method**: We will spin up the Axum server on a random available port (or fallback to a specific port) during Tauri's `setup` phase using `tokio::spawn`. Once the port is bound and the server is listening, we will programmatically construct or update the Tauri main window to point to `http://localhost:{port}`.
- **Project Structure & Feature Flags**: Instead of introducing a new `src-tauri` directory, we will integrate Tauri directly into the existing `esdiag` binary. To ensure standalone CLI users aren't burdened by GUI dependencies, all Tauri code, configuration (`tauri.conf.json`, `build.rs` changes), and dependencies will be guarded by a new `desktop` cargo feature flag. The workspace will not include this feature by default.
- **Frontend Assets**: Instead of packaging a separate Vite or Node.js frontend, we will configure the Tauri application to rely on the server-rendered Askama templates and static assets served by Axum.

## Risks / Trade-offs

- **[Risk] Port Collisions**: Hardcoding a port might collide with other services running on the host machine.
  - *Mitigation*: Dynamically find an open port (using `std::net::TcpListener::bind("127.0.0.1:0")`) for the Axum server before launching the Tauri window.
- **[Risk] Process Lifecycle Sync**: The Axum server might not shut down gracefully when the Tauri window is closed.
  - *Mitigation*: Ensure the spawned Tokio task for Axum is either dropped or explicitly cancelled during Tauri's teardown event.
- **[Trade-off] Bundle Size**: Including Tauri increases the compiled binary size compared to the raw CLI or Axum server. This is acceptable for providing a native GUI experience.
