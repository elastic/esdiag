## Context

`/api/api_key` supports `?wait_for_completion` for synchronous processing: when set, it builds a `KnownHost` from the API key and URL, then runs the full `Processor` pipeline inline, returning `diagnostic_id`, `kibana_link`, and `took`. `/api/service_link` follows the identical pattern for async only — it validates the upload service URL, tokenizes credentials, and stashes the `Uri` in server state for later retrieval via the web UI.

The two handlers diverge only in credential handling (API key vs token in URL) and the intermediate type (`KnownHost` vs `Uri`). The `Receiver` can already be constructed from either. The `Processor` pipeline is identical once a `Receiver` exists.

The documentation also contains several inaccuracies (wrong field name, broken link, type errors, invalid JSON) catalogued in the proposal.

## Goals / Non-Goals

**Goals:**
- Add `?wait_for_completion` to `/api/service_link` with identical semantics and response shape as `/api/api_key`
- Fix all catalogued documentation inaccuracies in `docs/api/`

**Non-Goals:**
- Changing the async default behavior of either endpoint
- Adding `wait_for_completion` to the web form handlers (`/service_link`, `/api_key`)
- Streaming or partial-result responses
- Timeout or cancellation support

## Decisions

### Reuse `deserialize_empty_as_true` and `ApiKeyQueryParams` pattern

**Decision**: Introduce a `ServiceLinkQueryParams` struct identical in shape to `ApiKeyQueryParams`, sharing the `deserialize_empty_as_true` deserializer already defined in `api.rs`.

**Rationale**: The three-value boolean (`?param`, `?param=true`, `?param=false`) is already tested and behaves correctly. Duplicating the struct keeps the handler self-contained and avoids a premature shared type. If a third endpoint needs this pattern, extraction to a shared type is straightforward then.

**Alternative considered**: Generic `WaitParams` struct shared across both handlers. Rejected — premature abstraction for two call sites.

### Inline sync path in `service_link` handler

**Decision**: Add the sync branch directly in `api::service_link` (in `src/server/api.rs`), mirroring the `api_key` sync branch structure.

**Rationale**: The `api_key` sync path is ~115 lines and self-contained. The service_link variant will be similarly scoped. Extracting a shared helper would require threading `&Arc<ServerState>`, `String` (request_user), `Identifiers`, `Uri`/`KnownHost` through a common interface — more complexity than value at this stage.

**Alternative considered**: Extract `run_sync_processor(state, receiver, identifiers) -> impl IntoResponse` helper. Viable but not worth the generalization now.

### Response shape for service_link sync

**Decision**: Return `{"diagnostic_id": ..., "kibana_link": ..., "took": ...}` — same as `api_key` sync — with HTTP 200.

**Rationale**: Callers using `wait_for_completion` on both endpoints can handle responses uniformly. The async response (`link_id`) is distinct and irrelevant in sync mode.

### Documentation field name: `kibana_link` not `kibana_url`

**Decision**: Update all documentation to use `kibana_link` (matching the actual JSON field in `src/server/api.rs:305`).

**Rationale**: The implementation uses `kibana_link`; the docs incorrectly say `kibana_url`. The implementation is authoritative.

## Risks / Trade-offs

- **Long-running requests**: A sync `service_link` call downloads a potentially large diagnostic zip from the Elastic Upload Service before processing. Clients must handle long timeouts. → No mitigation required; callers choosing `wait_for_completion` opt into this.
- **Sync path code duplication**: The sync handler block will be structurally similar to the `api_key` sync path. → Acceptable until a third endpoint warrants extraction.
- **No rollback needed**: The async path is unchanged; `wait_for_completion` defaults to `false`. Removing the feature later is a one-line handler change.
