## 1. Service Link Sync Implementation

- [x] 1.1 Add `ServiceLinkQueryParams` struct to `src/server/api.rs` with `wait_for_completion: bool` using the existing `deserialize_empty_as_true` deserializer
- [x] 1.2 Add `Query(params): Query<ServiceLinkQueryParams>` extractor to the `service_link` handler signature
- [x] 1.3 Implement the sync branch in `service_link`: when `params.wait_for_completion`, build a `Receiver` from the tokenized `Uri`, create and start the `Processor`, await completion, and return `{"diagnostic_id", "kibana_link", "took"}` with HTTP 200
- [x] 1.4 Implement the async branch: wrap existing stash-and-return-`link_id` logic in the `else` branch

## 2. Documentation Fixes

- [x] 2.1 Fix `docs/api/types.md`: rename `kibana_url` → `kibana_link` in the ApiKey Response (Synchronous) section
- [x] 2.2 Fix `docs/api/types.md`: change `link_id` field type description from `String` to `Integer`
- [x] 2.3 Fix `docs/api/types.md`: correct `case_number` type to `"string | null"` in all locations (matches `Option<String>` in `Identifiers`)
- [x] 2.4 Fix `docs/api/types.md`: change file size limit from `512 GiB` to `512 MiB`
- [x] 2.5 Fix `docs/api/types.md`: clarify HTTP 201 status code description to cover both `/api/service_link` and `/api/api_key` (async)
- [x] 2.6 Fix `docs/api/examples.md`: remove trailing comma after `"opportunity": null,` in the `service_link` request example
- [x] 2.7 Fix `docs/api/examples.md`: correct `case_number` examples to use quoted strings (e.g. `"98765"`) matching `Option<String>` type
- [x] 2.8 Fix `docs/api/examples.md`: change `link_id` in workflow response example from string `"45678"` to integer `456789`
- [x] 2.9 Fix `docs/api/examples.md`: rename `kibana_url` → `kibana_link` in all synchronous response examples
- [x] 2.10 Fix `docs/api/README.md`: remove broken reference to non-existent `endpoints.md`
- [x] 2.11 Update `docs/api/README.md`: add `wait_for_completion` query parameter documentation for `/api/service_link`, describing sync and async modes and their respective response shapes

## 3. Verification

- [x] 3.1 Run `cargo clippy` and resolve any warnings in modified files
- [x] 3.2 Run `cargo test` and confirm all tests pass
