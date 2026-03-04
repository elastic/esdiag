## ADDED Requirements

### Requirement: Channel-Driven Web Event Publication
The Web UI streaming backend MUST publish observable UI state updates through Tokio channel primitives and MUST convert channel messages into valid SSE payloads for Datastar-compatible consumers.

#### Scenario: Publishing an update from internal state
- **WHEN** a web-visible state update is produced by the server
- **THEN** the update is sent through the configured Tokio channel and emitted as a valid SSE event record.

### Requirement: Shared Event Bus For Non-Processor UI Mutations
Endpoints that mutate UI state outside long-lived processor tasks MUST publish their UI mutations to the shared `/events` stream and MAY return `204 No Content` when no direct payload response is required.

#### Scenario: Non-processor UI mutation endpoint is invoked
- **WHEN** a non-processor action endpoint updates UI-related state
- **THEN** the endpoint publishes Datastar-compatible mutations to the shared event bus and is permitted to return `204 No Content`.

### Requirement: Focus-Aware Session Stream Behavior
The `/events` stream MUST act as a session-scoped real-time stream that follows Datastar focus behavior, including disconnect when the browser tab is not focused and resume on focus return.

#### Scenario: Session stream while tab loses focus
- **WHEN** a client tab transitions to an unfocused state
- **THEN** the session stream disconnects without affecting server-side processing jobs.

#### Scenario: Session stream when tab regains focus
- **WHEN** a client tab returns to focused state
- **THEN** the session stream reconnects and resumes receiving current activity/stat updates.

### Requirement: Snapshot-Then-Incremental Session Semantics
For session streams that expose current UI state, the server MUST send an initial current-state event before subsequent incremental updates for each new subscriber.

#### Scenario: Client subscribes to a state stream
- **WHEN** a client opens a new SSE subscription
- **THEN** the first emitted event represents current state, followed by incremental updates in source order.

### Requirement: Processing Stream Runs To Completion
The processing stream used for active collect/process work (including `/upload/process`) MUST remain active for the lifetime of the job and MUST only terminate on terminal job completion (success/failure), explicit unrecoverable error, or server shutdown.

#### Scenario: Long-running processing task during tab focus change
- **WHEN** a collect/process job is in progress and the client tab loses focus
- **THEN** the server continues processing the job and maintains stream-state continuity for completion reporting.

#### Scenario: Processing stream reaches successful completion
- **WHEN** the active job finishes successfully
- **THEN** the stream emits completion state and then closes.

### Requirement: Deterministic Stream Termination
Web SSE streams MUST terminate cleanly when shutdown is requested, the source channel closes, or endpoint-specific terminal conditions are reached.

#### Scenario: Server shutdown while clients are subscribed
- **WHEN** server shutdown is initiated
- **THEN** active stream handlers stop publishing and complete without leaving background stream tasks running.

### Requirement: Datastar Event Framing Compatibility
The `/events` stream MUST preserve Datastar-compatible framing and HTTP `text/event-stream` response semantics for published UI mutations.

#### Scenario: Streaming response from refactored endpoint
- **WHEN** a client receives events from `/events`
- **THEN** each message is parseable as Datastar SSE data and the response content type is `text/event-stream`.
