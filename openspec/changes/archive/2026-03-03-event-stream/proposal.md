## Why

The current web event-stream handlers rely on `async-stream` macros, which make return types and control flow less ergonomic than necessary for simple SSE publishing. Recent experimentation shows we can model the same behavior with Tokio channels and direct SSE event production, reducing complexity and clarifying ownership and shutdown semantics.

## What Changes

- Introduce a channel-driven event stream pattern for the web interface where publishers emit typed events and SSE handlers adapt those events into Datastar-compatible responses.
- Replace `async-stream` usage in web-facing stream handlers with Tokio-native channel/receiver flows and explicit event mapping.
- Standardize stream lifecycle behavior (initial snapshot, incremental updates, disconnect, and shutdown) using explicit channel and cancellation boundaries.
- Remove the `async-stream` dependency from server-side event streaming paths when no longer needed.

## Capabilities

### New Capabilities
- `web-event-streaming`: Define channel-driven SSE behavior for web state updates, including event ordering, lifecycle handling, and Datastar event compatibility.

### Modified Capabilities
- `ui-theming`: Ensure theme updates continue to return valid `text/event-stream` Datastar events after event streaming internals are refactored.

## Impact

- Affected code: web server stream handlers in `src/server/*` (especially Datastar/SSE response paths) and related stream-producing components.
- Dependencies: potential removal of `async-stream` from `Cargo.toml` and lockfile once all relevant usage is migrated.
- Runtime behavior: stream update ordering, keep-alive cadence, and shutdown/disconnect behavior must remain stable.
- Surface area: Web UI streaming behavior changes; no CLI behavior changes intended.
