## Context

`esdiag` currently uses `async-stream` in several modules, including web server paths that emit Datastar-compatible `text/event-stream` responses. For the web interface, the stream lifecycle is conceptually simple: produce an initial state snapshot, forward subsequent updates, and stop cleanly on disconnect/shutdown. In `datastar-matrix`, this same behavior is modeled with Tokio primitives (broadcast/mpsc channels plus cancellation) and explicit event conversion, which is easier to reason about than macro-generated stream blocks.

This change focuses on web-facing event streaming ergonomics. CLI and non-web processing behavior remain unchanged.

## Goals / Non-Goals

**Goals:**
- Establish a channel-driven pattern for web event streaming that uses Tokio senders/receivers as the primary transport between producers and SSE handlers.
- Preserve current web behavior: valid Datastar event framing, stable event ordering, and clean shutdown/disconnect handling.
- Reduce dependence on stream-construction macros in web SSE code paths.
- Define migration boundaries so the dependency can be removed once all relevant web handlers are migrated.

**Non-Goals:**
- Re-architecting core receiver/processor/exporter type-state flows.
- Changing user-facing theme semantics, page structure, or CLI output formats.
- Introducing external brokers or runtime services.

## Decisions

1. **Adopt channel-first stream composition for web SSE handlers**
   - Web publishers emit typed update events into bounded Tokio `mpsc` channels by default for web endpoints.
   - SSE endpoints consume receiver messages and map them into Datastar-compatible event payloads.
   - **Why:** this mirrors existing Tokio-first architecture, keeps ownership boundaries explicit, and simplifies reasoning about cancellation.
   - **Alternatives considered:**
     - Keep `async-stream`: lower immediate churn but retains less ergonomic control-flow and return signatures.
     - Switch to `asynk-strim`: could improve ergonomics over `async-stream`, but still adds a dedicated stream-construction layer where channels already solve the use case.
     - Use `broadcast` by default: useful for high fanout, but unnecessary overhead for expected low concurrent user activity.

2. **Model two stream classes with different lifecycle guarantees**
   - **Session stream (`/events`)**: focus-aware real-time updates that follow Datastar behavior (disconnect when tab is unfocused, reconnect/resume when focus returns).
   - **Processing stream (`/upload/process` and similar collect/process flows)**: long-lived job stream that remains active until terminal job state (success/failure) and is not cancelled due to focus changes.
   - **Why:** these streams represent different product semantics; conflating them risks incorrect cancellation and incomplete job feedback.
   - **Alternatives considered:** one uniform lifecycle policy for all streams (rejected because processing streams require stronger completion guarantees).

3. **Keep Datastar framing as the protocol contract**
   - Event mapping remains responsible for generating valid Datastar SSE event records on the shared `/events` stream (`text/event-stream` payload semantics).
   - Non-processor action endpoints can return `204 No Content` and publish UI mutations to `/events`; theme updates continue setting cookies.
   - **Why:** keeps frontend behavior stable while allowing backend refactor.

4. **Stage migration by web module, then remove dependency**
   - Migrate web streaming handlers first.
   - Confirm no web SSE path needs `async-stream`.
   - Remove dependency only after usage reaches zero in targeted paths.
   - **Why:** minimizes risk and enables incremental validation.

## Risks / Trade-offs

- **[Risk] Channel backpressure or lag behavior changes under load** -> Mitigation: define per-stream channel type/capacity explicitly and add tests for lag/drop handling semantics.
- **[Risk] Ordering regressions between snapshot and incremental events** -> Mitigation: codify ordering requirements in specs and add integration tests that assert first-event behavior.
- **[Risk] Subtle disconnect/shutdown leaks** -> Mitigation: require explicit termination conditions per stream class and add teardown-focused tests.
- **[Risk] Processing stream dropped on tab focus transitions** -> Mitigation: decouple job lifecycle from UI focus lifecycle and define reconnect behavior that resumes observing in-flight job state.
- **[Trade-off] More explicit state plumbing in handlers** -> Mitigation: centralize mapping helpers and shared lifecycle utilities to keep endpoint code concise.

## Migration Plan

1. Inventory web SSE handlers and classify fanout requirement (single-consumer vs multi-subscriber).
2. Introduce shared event types + mapping helpers from internal update to Datastar SSE payload.
3. Refactor targeted handlers to receiver-driven loops with stream-class-specific lifecycle handling.
4. Add/adjust tests for snapshot-first semantics, update ordering, focus-aware session behavior, and processing-stream completion guarantees.
5. Remove `async-stream` from web paths and then from dependencies once no longer required there.
6. Rollback approach: keep previous handler implementation available per module until parity tests pass.

## Open Questions

- Do we want a shared stream utility abstraction in `src/server` to avoid duplicating loop/lifecycle logic across handlers?
  - Candidate reusable logic includes: snapshot-first emission, update loop + ordering guardrails, stream-class termination policy (focus-aware vs run-to-completion), keep-alive configuration, and uniform error-to-event mapping.
  - Candidate reusable helpers include: `stream_from_receiver(...)`, `emit_snapshot_then_updates(...)`, `session_stream_policy(...)`, `processing_stream_policy(...)`, and common event mappers for Datastar SSE framing.
