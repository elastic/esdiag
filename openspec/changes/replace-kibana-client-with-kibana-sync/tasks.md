## 1. Dependency and Compatibility Layer

- [x] 1.1 Add `kibana-sync = "0.2.0"` to `Cargo.toml` and update the lockfile.
- [x] 1.2 Add an explicit conversion from `crate::data::Auth` to `kibana_sync::Auth`.
- [x] 1.3 Define or reuse the existing Kibana request concurrency limit and pass it to `kibana_sync::KibanaClientBuilder::max_concurrency`.
- [x] 1.4 Replace the local `src/client/kibana.rs` implementation with a wrapper or re-export backed by `kibana_sync::KibanaClient`.
- [x] 1.5 Preserve existing `TryFrom<KnownHost>`, `Display`, `request`, and `test_connection` call-site behavior or update call sites to use the shared client directly.

## 2. Receiver Migration

- [x] 2.1 Update `KibanaReceiver` construction to build the shared Kibana client from saved host URL and auth values.
- [x] 2.2 Replace local version parsing/status handling with `kibana-sync` client helpers where they preserve ESDiag's manifest output.
- [x] 2.3 Keep `get_spaces()`, `get_raw_response_by_path()`, `Receive`, and `ReceiveRaw` behavior stable, including response timing and byte-size accounting.
- [x] 2.4 Add or update unit tests for Basic, API key, no-auth, and concurrency-limit client construction.

## 3. Collector and Error Handling

- [x] 3.1 Update Kibana retry classification to recognize transport failures wrapped in `kibana_sync::Error`.
- [x] 3.2 Verify non-success HTTP responses still produce `KibanaRequestError` with status, body, response time, and response size.
- [x] 3.3 Add unit coverage proving space-aware request paths are prefixed exactly once during collection.
- [x] 3.4 Confirm multipart saved-object request behavior still uses the shared client's multipart implementation.

## 4. Regression Coverage

- [x] 4.1 Run focused Kibana receiver/client tests.
- [x] 4.2 Run `cargo test` for the project or the broadest feasible test subset.
- [x] 4.3 Run `cargo clippy` and address actionable warnings introduced by the migration.
- [x] 4.4 Run ignored external Kibana compatibility tests when suitable Kibana `6.8.x`, `7.17.x`, `8.19.x`, and `9.x` targets are available. Not run here; no external matrix targets are configured in this environment.

## 5. Bundled Kibana Asset Setup

- [x] 5.1 Convert `assets/kibana` from the flat `assets.yml`/NDJSON setup format to the `kibana-sync` filesystem bundle layout.
- [x] 5.2 Preserve the full ESDiag Kibana space definition in the bundle while adding root and per-space manifests.
- [x] 5.3 Split bundled saved objects into per-object JSON resources and generate `saved_objects.json` from their type/id pairs.
- [x] 5.4 Generate a single Kibana asset bundle at build time and exclude raw `kibana/**` files from the generic embedded assets tree.
- [x] 5.5 Update `esdiag setup` to read Kibana setup assets from the generated bundle and reconstruct the saved-object import payload.
- [x] 5.6 Add unit coverage for `KibanaFsBundle` layout compatibility, bundle-only embedding, full space definition preservation, and NDJSON reconstruction.
- [x] 5.7 Run `esdiag-control up --insecure --open-browser false --debug` on `ironhide` and verify Kibana imported the `esdiag` space plus all 90 bundled saved objects.

## 6. Documentation and Release Notes

- [x] 6.1 Update nearby docs only if implementation behavior or dependency guidance changes.
- [x] 6.2 Decide whether `CHANGELOG.md` needs an entry; add one only if the migration creates a user-visible behavior change.
- [x] 6.3 Re-run OpenSpec validation/status checks and ensure all tasks are ready for apply.
