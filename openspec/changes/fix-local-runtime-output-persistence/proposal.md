## Why

The standalone local stack starts the ESDiag web container in User mode while providing its Elasticsearch output URL and API key through the runtime environment. User mode currently ignores that configured output at startup and falls back to the stdout exporter, causing processed documents to be written to container logs; local user artifacts are also lost with the container because its `~/.esdiag` state is not persistent.

## What Changes

- Make User-mode `serve` startup honor a complete runtime-provided Elasticsearch output before falling back to stdout.
- Fail startup when runtime output configuration is present but invalid instead of silently selecting another exporter.
- Keep the standalone deployment in User mode so local UI features remain available without service-mode IAP requirements.
- Leave the UI output unset by default so the existing environment-backed exporter fallback uses the generated local API key without requiring an encrypted keystore or unlock prompt.
- Persist ESDiag User-mode artifacts across container replacement with a dedicated Compose volume, preserving them through `down` and removing them through confirmed `reset`.
- Make the local runtime mode explicit in generated Compose configuration and finish verification with an API-key job that diagnoses `http://elasticsearch:9200` into that same local output, priming lazily created mapping fields while proving documents reach Elasticsearch rather than stdout.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `standalone-local-stack`: Require a persistent ESDiag user-state volume and a working environment-backed default Elasticsearch output in the generated local stack.
- `web-runtime-modes`: Define runtime-provided output precedence for User mode while retaining local single-user features.
- `web-secure-processing-gate`: Exempt runtime-authenticated User-mode outputs from keystore bootstrap and unlock requirements.

## Impact

- Affects Elasticsearch output selection in the Rust CLI `serve` startup path and User-mode web processing.
- Affects generated Compose and lifecycle behavior in `bin/esdiag-local`, including persistent volumes and reset semantics.
- Affects the ESDiag container, local Elasticsearch exporter, keystore preflight, release image, release notes, and standalone integration coverage.
- Requires rebuilding the 0.16 multi-platform container images and regenerating the draft `esdiag-local` release assets before publication.
