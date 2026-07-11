## 1. Runtime Exporter Resolution

- [x] 1.1 Remove the User-mode stdout override so omitted `serve` output uses the existing `ESDIAG_OUTPUT_*` fallback.
- [x] 1.2 Present runtime output only as `Default`, submit it as `null`/`None`, resolve that absence through `ESDIAG_OUTPUT_*`, and list only saved hosts as additional explicit remote outputs.
- [x] 1.3 Fail with an actionable, secret-safe error when neither the UI nor the environment provides a valid output.
- [x] 1.4 Add unit tests for explicit-output precedence, environment fallback, missing or invalid environment failure, and no stdout fallback.

## 2. Keystore And Settings Integration

- [x] 2.1 Limit keystore preflight to an explicitly selected saved output so an unset UI output can use runtime environment authentication.
- [x] 2.2 Preserve existing settings-driven exporter validation and updates without adding credential-origin state.
- [x] 2.3 Add regression coverage for an unset UI output whose environment-backed URL matches a saved secure host.
- [x] 2.4 Verify existing secure saved-host processing still blocks on a locked keystore and proceeds after unlock.

## 3. Standalone Persistent State

- [x] 3.1 Generate `ESDIAG_MODE=user` explicitly for the standalone ESDiag service.
- [x] 3.2 Add a dedicated `esdiag-data` named volume mounted at `/root/.esdiag` and preserve it through `down` and service recreation.
- [x] 3.3 Extend lifecycle state and reset handling so the new volume is recognized, added safely to pre-release existing deployments, and removed by `reset --force`.
- [x] 3.4 Extend `tests/esdiag-local.sh` and Compose validation for runtime output variables, explicit User mode, persistent ESDiag state, and reset behavior.
- [x] 3.5 Keep the setup container on the internal Kibana service URL while configuring the web container with the browser-reachable `localhost` URL, with generated-Compose regression coverage.

## 4. End-To-End Behavior And Documentation

- [x] 4.1 Add an integration test that submits a synchronous API-key processing job through the local web service with `http://elasticsearch:9200` as both source and runtime output, uses the generated API key, and verifies the self-diagnostic documents are indexed locally rather than streamed to container stdout.
- [x] 4.2 Verify setup and web containers share the generated runtime API key without writing it to `hosts.yml`, `settings.yml`, or `secrets.yml`.
- [x] 4.3 Update standalone, runtime-mode, output, and keystore documentation to describe runtime-managed default output and persistent User-mode state.
- [x] 4.4 Create the remote-processing job entry before fallible receiver/exporter setup so connection and configuration failures remain visible in the UI job feed.
- [ ] 4.5 Update `CHANGELOG.md` and the curated 0.16 release notes with the corrected local export and persistence behavior.

## 5. Verification And Release Refresh

- [x] 5.1 Run `shellcheck bin/esdiag-local`, standalone/control shell suites, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- [x] 5.2 Run strict OpenSpec validation for `fix-local-runtime-output-persistence` and reconcile any implementation/spec mismatch.
- [x] 5.3 As the final runtime test and system primer, run the live synchronous API-key job from and to `http://elasticsearch:9200`, confirm it returns a diagnostic identifier, verify its documents and lazily materialized mapping fields in local Elasticsearch, and verify no processed document stream appears on container stdout.
- [ ] 5.4 Rebuild and push identical 0.16 multi-platform image aliases to both Elastic registries, verify digests and architectures, and confirm the image reports version 0.16.0.
- [ ] 5.5 With explicit approval, move the unpublished `0.16.0` tag to the verified release commit, rerun the draft workflow, and confirm corrected notes and standalone assets while leaving the release unpublished.
