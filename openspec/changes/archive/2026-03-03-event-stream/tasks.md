## 1. Stream Inventory and Interface Design

- [x] 1.1 Identify web-facing SSE/datastar endpoints in `src/server` that currently depend on `async-stream`.
- [x] 1.2 Define typed internal event payloads and channel strategy (bounded `mpsc` default) for session and processing stream classes.
- [x] 1.3 Add shared event-to-Datastar mapping helpers for consistent `text/event-stream` framing.

## 2. Channel-Driven Handler Refactor

- [x] 2.1 Refactor `/events` to consume Tokio receivers and enforce focus-aware session stream behavior with reconnect/resume semantics.
- [x] 2.2 Refactor `/upload/process` (and related collect/process task streams) to remain active until terminal job state, independent of tab focus transitions.
- [x] 2.3 Implement explicit termination paths for shutdown, source-channel closure, and endpoint-specific terminal conditions.
- [x] 2.4 Preserve/verify existing keep-alive and content-type behavior on migrated endpoints.

## 3. Theme Streaming Compatibility

- [x] 3.1 Ensure `ui-theming` toggle responses remain Datastar-compatible event sequences after stream internal changes.
- [x] 3.2 Validate theme cookie + signal patch behavior remains unchanged for web and desktop modes.

## 4. Dependency Cleanup and Verification

- [x] 4.1 Remove `async-stream` from migrated web paths and clean dependency declarations once no longer needed there.
- [x] 4.2 Add or update tests for snapshot-first semantics, event ordering, focus-aware `/events` lifecycle, and run-to-completion `/upload/process` lifecycle.
- [x] 4.3 Run `cargo clippy` and address any new warnings introduced by the refactor.
- [x] 4.4 Run `cargo test` and confirm web streaming/theming behavior is covered and passing.
