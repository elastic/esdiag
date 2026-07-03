## Why

The Elastic CLI now includes an experimental extension system, and ESDiag should be invokable as `elastic diag ...` without forcing users to duplicate Elastic CLI context into `~/.esdiag/hosts.yml` and the ESDiag keystore. Registering ESDiag as a first-class extension gives diagnostics collection and processing a natural home in the new `elastic` workflow while preserving the existing standalone `esdiag` CLI.

## What Changes

- Add an Elastic CLI extension entrypoint named `elastic-diag` that forwards `elastic diag <args...>` to the existing ESDiag execution layer.
- Support Elastic CLI context environment variables as runtime inputs for Elasticsearch and Kibana targets, including API key and basic auth values.
- Support active-context target references such as `.es` and `.kb` when running through the Elastic CLI extension.
- Add native Elastic CLI config support for named-context references such as `.prod.es` and `.diag.elasticsearch`.
- Document and package the extension so it can be installed through the Elastic CLI extension lifecycle and discovered as a diagnostics extension.
- Preserve existing `esdiag` command names, flags, host files, keystore behavior, and environment variables.
- Add tests covering extension command forwarding, Elastic CLI environment mapping, and the env-backed diagnostic output path.

## Capabilities

### New Capabilities
- `elastic-cli-extension`: Defines how ESDiag is exposed through the Elastic CLI extension system, including extension naming, command forwarding, and Elastic CLI context consumption.
- `elastic-cli-config`: Defines how ESDiag reads Elastic CLI configuration for named-context target references.

### Modified Capabilities

## Impact

- Affects CLI and packaging behavior; Web UI behavior and core diagnostic processing semantics remain unchanged.
- Adds an extension entrypoint and supporting docs/package metadata for the Elastic CLI extension system.
- Updates environment resolution for Elasticsearch and Kibana targets so `ELASTIC_*` variables can be used alongside existing `ESDIAG_*` variables.
- Adds a parser and resolver for Elastic context target references in CLI arguments that already support remote targets.
- May require release or repository metadata updates, such as using the `elastic-extension` GitHub topic or a package/repository name that installs as `diag`.
