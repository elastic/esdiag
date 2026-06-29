## 1. Dependency and Compatibility Layer

- [ ] 1.1 Add `kibana-sync = "0.1.0"` to `Cargo.toml` and update the lockfile.
- [ ] 1.2 Add an explicit conversion from `crate::data::Auth` to `kibana_sync::Auth`.
- [ ] 1.3 Define or reuse the existing Kibana request concurrency limit and pass it to `kibana_sync::KibanaClientBuilder::max_concurrency`.
- [ ] 1.4 Replace the local `src/client/kibana.rs` implementation with a wrapper or re-export backed by `kibana_sync::KibanaClient`.
- [ ] 1.5 Preserve existing `TryFrom<KnownHost>`, `Display`, `request`, and `test_connection` call-site behavior or update call sites to use the shared client directly.

## 2. Receiver Migration

- [ ] 2.1 Update `KibanaReceiver` construction to build the shared Kibana client from saved host URL and auth values.
- [ ] 2.2 Replace local version parsing/status handling with `kibana-sync` client helpers where they preserve ESDiag's manifest output.
- [ ] 2.3 Keep `get_spaces()`, `get_raw_response_by_path()`, `Receive`, and `ReceiveRaw` behavior stable, including response timing and byte-size accounting.
- [ ] 2.4 Add or update unit tests for Basic, API key, no-auth, and concurrency-limit client construction.

## 3. Collector and Error Handling

- [ ] 3.1 Update Kibana retry classification to recognize transport failures wrapped in `kibana_sync::Error`.
- [ ] 3.2 Verify non-success HTTP responses still produce `KibanaRequestError` with status, body, response time, and response size.
- [ ] 3.3 Add unit coverage proving space-aware request paths are prefixed exactly once during collection.
- [ ] 3.4 Confirm multipart saved-object request behavior still uses the shared client's multipart implementation.

## 4. Regression Coverage

- [ ] 4.1 Run focused Kibana receiver/client tests.
- [ ] 4.2 Run `cargo test` for the project or the broadest feasible test subset.
- [ ] 4.3 Run `cargo clippy` and address actionable warnings introduced by the migration.
- [ ] 4.4 Run ignored external Kibana compatibility tests when suitable Kibana `6.8.x`, `7.17.x`, `8.19.x`, and `9.x` targets are available.

## 5. Documentation and Release Notes

- [ ] 5.1 Update nearby docs only if implementation behavior or dependency guidance changes.
- [ ] 5.2 Confirm the implementation leaves bundled Kibana asset expansion as a separate follow-up built on the shared `kibana-sync` dependency.
- [ ] 5.3 Decide whether `CHANGELOG.md` needs an entry; add one only if the migration creates a user-visible behavior change.
- [ ] 5.4 Re-run OpenSpec validation/status checks and ensure all tasks are ready for apply.
