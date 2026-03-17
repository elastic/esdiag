## Why

The `/api/service_link` endpoint only supports asynchronous processing, requiring callers to redirect users to the web UI to monitor progress — a friction point for programmatic integrations. The `/api/api_key` endpoint already supports `?wait_for_completion` for synchronous use; `service_link` should have parity. Additionally, a sweep of the API documentation revealed several inaccuracies that would mislead integrators.

## What Changes

- **`/api/service_link`**: Accept a `?wait_for_completion` query parameter (same semantics as `/api/api_key`). When `true`, process the service link synchronously and return `diagnostic_id`, `kibana_link`, and `took`. When `false` or absent, preserve current async behavior returning `link_id`.
- **`docs/api/README.md`**: Remove broken reference to non-existent `endpoints.md`; document `wait_for_completion` for `service_link`.
- **`docs/api/types.md`**: Fix `kibana_url` → `kibana_link` (matches actual response field); fix `link_id` type `String` → `Integer`; fix `case_number` type to `string | null` (matches `Option<String>` in `Identifiers`); fix file size limit `512 GiB` → `512 MiB`; clarify 201/200 status code usage.
- **`docs/api/examples.md`**: Fix trailing comma in `service_link` JSON example (invalid JSON); fix `case_number` examples to use quoted strings (matches `Option<String>` type); fix `link_id` shown as string `"45678"` → integer `456789`; rename `kibana_url` → `kibana_link` in response examples; add synchronous `service_link` examples.

## Capabilities

### New Capabilities

- `service-link-wait`: Synchronous processing mode for the `/api/service_link` endpoint via `?wait_for_completion`, mirroring the existing `api_key` sync path and returning the same diagnostic result shape.

### Modified Capabilities

<!-- No existing spec-level requirements are changing. The api_key sync behavior is already implemented; this extends it to service_link. -->

## Impact

- **Web server** (`src/server/api.rs`): Add `ServiceLinkQueryParams` struct and `wait_for_completion` branch to `service_link` handler, following the pattern established in `api_key`.
- **Documentation** (`docs/api/`): Three files updated — `README.md`, `types.md`, `examples.md`.
- **No breaking changes**: Default behavior of `service_link` is unchanged (async, returns `link_id`).
- **Affected systems**: Elasticsearch diagnostic pipeline (Web interface, service mode API).
