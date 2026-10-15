## Why

The Elastic CLI now includes an experimental extension system, and ESDiag should be invokable as `elastic diag ...` without forcing users to duplicate Elastic CLI context into `~/.esdiag/hosts.yml` and the ESDiag keystore. Registering ESDiag as a first-class extension gives the Collect and Process stages a natural home in the new `elastic` command surface while preserving the existing standalone `esdiag` CLI.

## What Changes

- Add an Elastic CLI invocation profile to the shared Rust execution layer under the intentionally shortened name `elastic diag`.
- Expose an extension-specific command profile while sharing ESDiag's Job construction and execution layer.
- Support Elastic CLI context environment variables as runtime-only inputs for Elasticsearch and Kibana Collect targets.
- Support active-context target references such as `.es` and `.kb` when running through the Elastic CLI extension.
- Add native Elastic CLI config support for named-context references such as `.prod.es` and `.diag.elasticsearch`.
- Package `elasticrc` as an independently publishable Rust library with a documented, ESDiag-independent API.
- Allow a named Elastic CLI context to be configured as the relatively fixed output deployment while input contexts change per command.
- Require explicit application selection and reject ambiguous commands such as a bare `elastic diag process`.
- Support deployment- and application-qualified Cloud-admin references such as `.prod.cloud/<deployment-id>/<application>` without treating Cloud as an application.
- Preserve backward compatibility for existing saved-host output configuration and saved Jobs when typed context references are introduced.
- Document the runtime contract consumed by a separately specified self-contained extension package.
- Preserve existing `esdiag` host files, keystore behavior, environment variables, and command behavior except for the intentional terminology change from `upload`/`--upload` to `send`/`--send`.
- Add tests covering the extension profile, active and named context resolution, configured output deployments, and Cloud-admin resource references.

## Capabilities

### New Capabilities
- `elastic-cli-extension`: Defines how ESDiag is exposed through the Elastic CLI extension system, including extension naming, command forwarding, and Elastic CLI context consumption.
- `elastic-cli-config`: Defines how ESDiag reads Elastic CLI configuration for named-context target references.

### Modified Capabilities

## Impact

- Affects CLI and packaging behavior; Web UI behavior and core diagnostic processing semantics remain unchanged.
- Adds an extension-specific command profile to the shared native execution layer.
- Adds typed resolution for active input contexts, named output deployments, and Cloud-admin resource references.
- Adds registry packaging, public documentation, semantic-versioning, and minimum-Rust-version obligations for `elasticrc`.
- Extends saved Job and application configuration models with symbolic Elastic CLI context references while keeping credentials runtime-only.
- Leaves release artifact selection, installer metadata, publication, and upgrades to an isolated follow-up change.
