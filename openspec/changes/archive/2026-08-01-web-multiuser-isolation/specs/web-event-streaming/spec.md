## MODIFIED Requirements

### Requirement: Shared Event Bus For Non-Processor UI Mutations
Endpoints that mutate UI state outside long-lived processor tasks MUST publish their UI
mutations to the shared `/events` stream and MAY return `204 No Content` when no direct
payload response is required. Every published mutation MUST carry an `Owner` and be
delivered only to that owner's subscribers; a mutation MUST NOT be broadcast to all
subscribers unless it belongs to the aggregate-stats broadcast class.

#### Scenario: Non-processor UI mutation endpoint is invoked
- **WHEN** a non-processor action endpoint updates UI-related state for a given owner
- **THEN** the endpoint publishes Datastar-compatible mutations to the shared event bus scoped to that owner and is permitted to return `204 No Content`
- **AND** the mutation is not delivered to other users' subscriptions

## ADDED Requirements

### Requirement: Owner-Scoped Event Visibility By Default
Every `ServerEvent` published to the `/events` stream SHALL carry an `Owner`, and event
visibility SHALL default to deny: an event is delivered to a subscriber only when the
event's owner equals the subscriber's resolved user or the event belongs to the
broadcast class. The visibility decision SHALL be `event.owner == user ||
event.is_broadcast()`, where `is_broadcast()` is true only for aggregate-stats events. A
new event variant that is neither owner-matched nor a broadcast-class event MUST NOT be
delivered to any other user, without requiring per-emission opt-in.

#### Scenario: Owner-scoped event reaches only its owner
- **WHEN** an owner-scoped event for user A is published
- **THEN** only subscribers resolved as user A MUST receive it
- **AND** subscribers resolved as any other user MUST NOT receive it

#### Scenario: Non-stats event is not broadcast by default
- **WHEN** a non-stats `ServerEvent` (for example a job-feed, template, or replace-selector update) is published for user A
- **THEN** it MUST NOT be delivered to user B, because it is neither owner-matched for B nor a broadcast-class event

### Requirement: Aggregate Stats Are The Sole Broadcast Class
The system SHALL broadcast the aggregate-`stats` class — server-wide processing state,
diagnostics processed, and document count — to all connected subscribers, and this class
SHALL be the only category for which `is_broadcast()` is true. Per-user job feeds, UI
mutations, and signals SHALL NOT be part of the broadcast class.

#### Scenario: Stats snapshot and updates reach all users
- **WHEN** a client subscribes to `/events`
- **THEN** it MUST receive the current aggregate-stats snapshot followed by aggregate-stats updates, regardless of which user owns the underlying jobs

#### Scenario: Only stats broadcast
- **WHEN** an event's `is_broadcast()` is evaluated
- **THEN** it MUST be true only for aggregate-stats events and false for every owner-scoped event

### Requirement: Execution Owner And Event Auto-Targeting
The system SHALL attach an `Owner` — the authenticated user who executed the job — to the
job execution, extending the existing retained-bundle owner onto the running job. A job's
emitted events SHALL be automatically targeted to that owner without per-emission opt-in.
When a diagnostic spawns included (child) diagnostics, the owner SHALL propagate to each
child alongside the existing parent and platform inheritance, so a child's events are
owner-scoped to the same executing user. Saved-job definitions SHALL require no owner for
isolation, because execution ownership is distinct from saved-job authorship.

#### Scenario: A job's events auto-target its executing owner
- **WHEN** a user executes a job
- **THEN** every event the job emits MUST be owner-scoped to that executing user without the emitting code opting in per event

#### Scenario: Owner propagates to child diagnostics
- **WHEN** a job's diagnostic spawns an included child diagnostic
- **THEN** the child's owner MUST equal the parent job's owner
- **AND** the child's events MUST be visible only to that owner

#### Scenario: Saved-job definition carries no execution owner
- **WHEN** a saved-job definition is authored
- **THEN** it MUST NOT require an owner for isolation, and its execution owner MUST be resolved from the user who later executes it
