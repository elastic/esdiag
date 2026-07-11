## 1. Runtime Exporter Resolution

- [ ] 1.1 Add explicit exporter credential-origin state for runtime-configured, local-keystore, and unsecured/stream outputs.
- [ ] 1.2 Update `serve` exporter resolution to apply explicit output, runtime environment, and mode fallback precedence without changing non-web CLI commands.
- [ ] 1.3 Detect partial or invalid `ESDIAG_OUTPUT_*` configuration and fail startup with actionable, secret-safe errors instead of falling back to stdout.
- [ ] 1.4 Add unit tests for explicit-output precedence, valid User-mode environment output, unconfigured User-mode stdout, invalid environment failure, and missing Service-mode output.

## 2. Keystore And Settings Integration

- [ ] 2.1 Initialize environment-backed Elasticsearch exporters with runtime credential origin and bypass keystore bootstrap for those exporters in User mode.
- [ ] 2.2 Update settings-driven exporter changes to validate and commit exporter plus credential origin atomically.
- [ ] 2.3 Add regression coverage for a runtime exporter whose URL matches a saved secure host and for switching from runtime output to a keystore-backed saved host.
- [ ] 2.4 Verify existing secure saved-host processing still blocks on a locked keystore and proceeds after unlock.

## 3. Standalone Persistent State

- [ ] 3.1 Generate `ESDIAG_MODE=user` explicitly for the standalone ESDiag service.
- [ ] 3.2 Add a dedicated `esdiag-data` named volume mounted at `/root/.esdiag` and preserve it through `down` and service recreation.
- [ ] 3.3 Extend lifecycle state and reset handling so the new volume is recognized, added safely to pre-release existing deployments, and removed by `reset --force`.
- [ ] 3.4 Extend `tests/esdiag-local.sh` and Compose validation for runtime output variables, explicit User mode, persistent ESDiag state, and reset behavior.

## 4. End-To-End Behavior And Documentation

- [ ] 4.1 Add an integration test that processes a representative diagnostic through the local web service and verifies documents are indexed in local Elasticsearch rather than streamed to container stdout.
- [ ] 4.2 Verify setup and web containers share the generated runtime API key without writing it to `hosts.yml`, `settings.yml`, or `secrets.yml`.
- [ ] 4.3 Update standalone, runtime-mode, output, and keystore documentation to describe runtime-managed default output and persistent User-mode state.
- [ ] 4.4 Update `CHANGELOG.md` and the curated 0.16 release notes with the corrected local export and persistence behavior.

## 5. Verification And Release Refresh

- [ ] 5.1 Run `shellcheck bin/esdiag-local`, standalone/control shell suites, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- [ ] 5.2 Run strict OpenSpec validation for `fix-local-runtime-output-persistence` and reconcile any implementation/spec mismatch.
- [ ] 5.3 Rebuild and push identical 0.16 multi-platform image aliases to both Elastic registries, verify digests and architectures, and confirm the image reports version 0.16.0.
- [ ] 5.4 With explicit approval, move the unpublished `0.16.0` tag to the verified release commit, rerun the draft workflow, and confirm corrected notes and standalone assets while leaving the release unpublished.
