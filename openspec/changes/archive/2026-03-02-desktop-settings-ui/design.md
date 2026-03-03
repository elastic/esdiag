## Context

The ESDiag app recently added a Tauri desktop capability. Since desktop applications aren't launched with command-line arguments or `ESDIAG_*` environment variables by average users, we need an intuitive way to configure the output destination (Elasticsearch cluster) and the Kibana URL.

## Goals / Non-Goals

**Goals:**
- Provide a Datastar-powered Settings modal to configure the `Exporter` target and Kibana URL.
- Leverage the existing `~/.esdiag/hosts.yml` infrastructure to allow selecting pre-configured hosts.
- Add a new `~/.esdiag/settings.yml` to persist the active selected host and URL across restarts.
- Allow updating the Axum `ServerState` dynamically when the settings are changed, without needing a full application restart.

**Non-Goals:**
- Removing CLI or environment variable support for the web/CLI usage modes.
- Building a full "Host Management" interface beyond just selecting existing hosts or saving new ones.

## Decisions

- **Configuration File (`settings.yml`)**: We will store active settings in `settings.yml` inside the standard `~/.esdiag` directory to match `hosts.yml`. We avoid JSON as per the user requirement.
- **Dynamic State Update**: The `ServerState` struct holds an `Arc<Exporter>`. When a user updates the target host via the Settings modal, the backend will construct a new `Exporter` and update the `ServerState`.
- **UI Integration**: We will add a "Settings" or "Target: <target_name>" link in the web footer. Clicking it will open an Askama-rendered modal. Credentials in the modal should use `type="password"` to keep secrets redacted.
- **Precedence**: When starting up, the system will check CLI args/env vars first (for CLI parity). If running in desktop mode or if variables are missing, it will attempt to load the active target from `settings.yml`.

## Risks / Trade-offs

- **[Risk] State Mutability**: Changing the `Exporter` mid-flight while an upload is actively processing could cause inconsistencies or panics if not synchronized properly.
  - *Mitigation*: Ensure the `ServerState.exporter` is behind a robust `RwLock` or that processors clone a static snapshot of the exporter state when a job starts.
- **[Trade-off] File I/O**: Reading/writing `settings.yml` introduces disk I/O on the backend for UI interactions, but it is minimal and isolated to user-triggered setting changes.
