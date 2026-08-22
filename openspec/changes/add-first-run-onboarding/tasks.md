## 1. Application Configuration Foundation

- [x] 1.1 Add a versioned `ApplicationConfig` model for `user`, `output.default` output-host reference, `output.authenticated_on` and `output.assets_version` metadata, and `job.default` saved-job reference with atomic `esdiag.yml` load/save behavior under the ESDiag state directory.
- [x] 1.2 Add validation that referenced output hosts and jobs exist, output hosts have role `send`, and output viewers resolve to role-`view` Kibana hosts without loading secret values into configuration.
- [x] 1.3 Add fixtures proving configuration round trips, rejects unknown versions and invalid references, and never serializes credential material.
- [x] 1.4 Keep existing web/desktop `settings.yml` reads and writes unchanged and prove CLI configuration loading does not mutate or infer preferences from that file.

## 2. Canonical Output Deployment

- [x] 2.1 Implement an atomic `OutputDeployment` resolver for explicit targets, complete environment deployments, and persisted output references in the specified precedence order.
- [x] 2.2 Reuse `ESDIAG_OUTPUT_*` authentication for both environment-backed Elasticsearch and Kibana clients and remove any need for analysis-specific credential resolution.
- [x] 2.3 Add resolution tests for explicit, environment, and persisted sources, including partial-environment failure and prevention of cross-source endpoint mixing.
- [x] 2.4 Migrate omitted-output CLI paths to the shared resolver without changing user-mode server/desktop startup or service-mode behavior.

## 3. Identity And Existing State Integration

- [x] 3.1 Apply `--user`, `ESDIAG_USER`, then `ApplicationConfig.user` precedence when constructing collection and processing identifiers.
- [x] 3.2 Expose flow-neutral typed services for configuration, deployment resolution, credential storage, host/job persistence, endpoint validation, setup, and readiness without embedding terminal prompts.
- [x] 3.3 Add tests for CLI omitted-output reuse, identifier precedence, and direct backend-service use independent of CLI presentation.

## 4. Initialization State Machine

- [x] 4.1 Add the `esdiag init` Clap surface and explicit inspect, identity, keystore, output, collection-host, default-job, and complete stages.
- [x] 4.2 Implement resumable existing-state inspection and output/workflow reuse, require explicit confirmation before replacing a default job, write `ApplicationConfig` only after required stages validate, and derive readiness from validated references rather than file existence.
- [x] 4.3 Implement hidden controlling-terminal prompts for keystore passwords and host credentials using existing keystore APIs, including non-TTY failure behavior and redaction tests.
- [x] 4.4 Implement output deployment creation/selection as a send-role Elasticsearch host linked to a view-role Kibana host, with shared-secret default and existing distinct-secret selection.
- [x] 4.5 Validate both output clients, treating approval to provision a new local core stack as approval for its lifecycle-owned assets while offering existing setup only after explicit approval for existing local and remote deployments.
- [x] 4.6 Implement the repeatable collect-host loop and first saved-job creation, defaulting to `Collect` plus `Process` with the configured send-role Elasticsearch host as its export target while retaining explicit collect-and-save-only selection.
- [x] 4.7 Emit the standard safe initialization outcome when structured CLI output is available and add interrupted/resumed end-to-end wizard tests.

## 5. Documentation

- [x] 5.1 Update CLI documentation, configuration documentation, and repository organization references for `esdiag.yml`, the initialization workflow, and the unchanged desktop settings boundary.
- [x] 5.2 Update `CHANGELOG.md` using the repository changelog skill without claiming that web/desktop onboarding or `settings.yml` migration is included.

## 6. Verification

- [x] 6.1 Run formatting and targeted configuration, host, keystore, identifier, saved-job, backend-service, and initialization tests.
- [x] 6.2 Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] 6.3 Run `cargo test --all-features`.
- [x] 6.4 Run strict OpenSpec validation for `add-first-run-onboarding`.

## 7. Verification Follow-ups

- [x] 7.1 Resume valid collection hosts by default, require confirmation before modifying existing collection, output, secret, default-output, or default-job state, and preserve configured state when replacement is declined.
- [x] 7.2 Require a keystore for processing readiness, validate default-job workflow shape, and report declined asset setup with recovery guidance.
- [x] 7.3 Correct persisted-output and linked-viewer documentation and add regression coverage for resumed readiness.
