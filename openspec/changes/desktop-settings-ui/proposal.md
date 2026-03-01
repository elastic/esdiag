## Why

With the introduction of the Tauri desktop application, users no longer have access to CLI arguments or environment variables to configure the destination cluster or Kibana URL when running ESDiag. To provide a smooth, standalone desktop experience, we need a native way for users to configure and persist these settings directly from the UI without relying on a terminal.

## What Changes

- Add a "Settings" configuration modal to the Web UI, accessible via the footer.
- The modal allows users to select an existing `KnownHost` (from `~/.esdiag/hosts.yml`) as their output destination, or create/save a new one.
- The modal allows users to configure a custom Kibana URL.
- Introduce a persistent `settings.yml` (stored alongside `hosts.yml`) to remember the user's active choices across application restarts.
- Add Axum API endpoints to read, write, and apply these settings dynamically without requiring a full server reboot.

## Capabilities

### New Capabilities
- `desktop-settings`: UI and backend capabilities for managing persistent user preferences (active exporter host, Kibana URL) without CLI flags.

### Modified Capabilities
- None.

## Impact

- **Code/APIs**: New Axum endpoints for fetching `KnownHosts` and saving active settings. The `ServerState` must be updated dynamically when the exporter target changes mid-session.
- **Web UI**: New Askama templates for the settings modal, integrated with Datastar for dynamic form submission and UI updates.
- **Dependencies**: Minor additions to `serde_yaml` usage if not already fully leveraged for reading/writing the new `settings.yml` file.
