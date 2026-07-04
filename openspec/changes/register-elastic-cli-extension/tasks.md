## 1. Environment Resolution

- [x] 1.1 Add helper logic for reading environment variables with `ESDIAG_*` precedence and `ELASTIC_*` fallbacks.
- [x] 1.2 Update Elasticsearch output URI resolution to accept `ELASTIC_ES_URL`, `ELASTIC_ES_API_KEY`, `ELASTIC_ES_USERNAME`, and `ELASTIC_ES_PASSWORD` when corresponding `ESDIAG_OUTPUT_*` values are absent.
- [x] 1.3 Update Kibana URI and link resolution to accept `ELASTIC_KIBANA_URL`, `ELASTIC_KIBANA_API_KEY`, `ELASTIC_KIBANA_USERNAME`, and `ELASTIC_KIBANA_PASSWORD` when corresponding ESDiag values are absent.
- [x] 1.4 Link `ELASTIC_CLOUD_*` context resolution into the existing ESDiag Cloud API key known-host code path.
- [x] 1.5 Add Rust tests for API key fallback, basic auth fallback, Kibana URL fallback, Cloud API key fallback, and `ESDIAG_*` precedence.

## 2. Active Context Target References

- [x] 2.1 Add a parser for active-context leading-dot target references with `.service` form.
- [x] 2.2 Map service aliases `es`, `kb`, and `cloud` to `elasticsearch`, `kibana`, and `cloud`.
- [x] 2.3 Resolve `.service` references from the active Elastic CLI context environment when available.
- [x] 2.4 Integrate active-context target resolution ahead of saved-host and local path resolution only when the service segment is a known service name or alias.
- [x] 2.5 Add tests for `.es`, `.kb`, `.cloud`, non-service leading-dot fallthrough, saved-host precedence behavior, and `./.es` local path handling.

## 3. Native Elastic CLI Config

- [x] 3.1 Convert the repository to a Cargo workspace while preserving the existing `esdiag` package and binary.
- [x] 3.2 Add a new `crates/elasticrc` library crate with public types for config files, contexts, service blocks, and resolved authentication.
- [x] 3.3 Add an `elasticrc` Cargo feature in the main ESDiag crate and include it in default features.
- [x] 3.4 Verify `cargo install --path .` remains functional from the repository root after workspace conversion.
- [x] 3.5 Add `keyring-core` and selected keyring-compatible native store crates to `elasticrc` for OS-backed secret resolution aligned with Elastic CLI behavior.
- [x] 3.6 Use `keyring` crate examples as implementation guidance for configuring native keyring stores.
- [x] 3.7 Add `redact` to protect resolved secret values in public typed structures and debug output.
- [x] 3.8 Implement Elastic CLI config discovery and loading in `elasticrc`.
- [x] 3.9 Support default config file discovery for `.elasticrc`, `.elasticrc.json`, `.elasticrc.yaml`, and `.elasticrc.yml`.
- [x] 3.10 Support `ELASTIC_CLI_CONFIG_FILE` and an explicit Elastic CLI config file override for ESDiag invocations.
- [x] 3.11 Reject executable config formats such as `.js`, `.ts`, `.mjs`, and `.cjs`.
- [x] 3.12 Validate config shape, context presence, service presence, HTTP(S) URLs, and supported auth blocks before constructing transient targets.
- [x] 3.13 Resolve `.context.service` references by parsing the rightmost segment as the service name or alias.
- [x] 3.14 Translate supported Elastic CLI API key, basic auth, Cloud API key, and unauthenticated service blocks into `elasticrc` resolved service blocks.
- [x] 3.15 Convert `elasticrc` resolved service blocks into transient ESDiag targets without persisting credentials.
- [x] 3.16 Implement Elastic CLI resolver expression support for `env`, `file`, `cmd`, `keychain`, `secret_service`, `pass`, and `credential_manager`, using `keyring-core` for OS-backed credential resolvers.
- [x] 3.17 Implement `cmd` and `pass` resolvers without shell interpretation, with explicit argv execution, bounded timeouts, clear errors, and Elastic CLI-style trust warnings.
- [x] 3.18 Emit inline-secret permission warnings when config files contain inline secrets and loose permissions.
- [x] 3.19 Add `elasticrc` crate tests for discovery, parsing, validation, expression resolution, platform-specific secret resolvers, inline secrets, permission warnings, redaction, and upstream schema drift fixtures.
- [x] 3.20 Add ESDiag integration tests for `.prod.es`, `.prod.elasticsearch`, `.prod.kb`, `.prod.cloud`, dotted context names, missing contexts, unsupported services, Elastic config precedence over saved hosts, and transient target conversion.

## 4. Extension Entrypoint

- [x] 4.1 Add an executable `elastic-diag` entrypoint that delegates all arguments to the ESDiag execution layer.
- [x] 4.2 Set `ESDIAG_ELASTIC_CLI=1` in the delegated ESDiag process environment.
- [x] 4.3 Ensure the entrypoint returns the delegated command exit status and does not log credential environment variable values.
- [x] 4.4 Detect when `esdiag` is missing from `PATH` and print clear Cargo install guidance.
- [x] 4.5 Add an integration or smoke test that verifies `elastic-diag process --help` reaches the same command surface as `esdiag process --help`.
- [x] 4.6 Add a test that verifies the delegated ESDiag process receives `ESDIAG_ELASTIC_CLI=1`.
- [x] 4.7 Make help context-aware when `ESDIAG_ELASTIC_CLI=1` is present and add tests for extension-specific help guidance.

## 5. Installation Metadata

- [x] 5.1 Add installer-compatible metadata so the Elastic CLI can discover the `elastic-diag` entrypoint from a GitHub clone.
- [x] 5.2 Keep packaging in the existing ESDiag repository without creating a separate extension repository.
- [x] 5.3 Document npm publication as deferred until local extension testing is complete.
- [x] 5.4 Document the initial PATH-based `esdiag` binary requirement and future precompiled-binary follow-up.

## 6. Documentation

- [x] 6.1 Update command-line documentation with Elastic CLI extension installation and usage examples.
- [x] 6.2 Document the mapping between `elastic diag <args...>` and `esdiag <args...>`.
- [x] 6.3 Document Elastic CLI context environment variables and the `ESDIAG_*` precedence rule.
- [x] 6.4 Document active `.service` references, named `.context.service` references, and supported service aliases.
- [x] 6.5 Use a remote Git repository install command in extension installation examples.
- [x] 6.6 Document resolver safety behavior, including no shell interpretation for command-backed resolvers and Elastic CLI-style trust warnings.
- [x] 6.7 Update `CHANGELOG.md` for the user-visible extension support.

## 7. Verification

- [x] 7.1 Run focused Rust tests for environment resolution, context target resolution, config loading, and CLI forwarding.
- [x] 7.2 Run `cargo test`.
- [x] 7.3 Run `cargo clippy`.
- [x] 7.4 Verify local Elastic CLI extension registration and `elastic diag --help`.
- [x] 7.5 Verify `cargo install --git https://github.com/elastic/esdiag.git` works for the documented initial binary install path when network access is available.
