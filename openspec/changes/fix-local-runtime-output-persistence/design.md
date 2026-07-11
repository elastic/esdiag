## Context

`esdiag-local` generates an Elasticsearch API key, persists it in its mode-0600 deployment `.env`, and passes `ESDIAG_OUTPUT_URL` and `ESDIAG_OUTPUT_APIKEY` to both setup and web containers. The web container runs `esdiag serve` without an explicit mode or output. Mode therefore defaults to User, while `resolve_serve_exporter` deliberately chooses `Exporter::default()` for User mode and ignores the environment. Because the default exporter is `Stream`, processed documents go to container stdout instead of Elasticsearch.

Switching the local container to Service mode is not viable: Service mode requires identity-aware-proxy headers and removes local host, saved-job, settings, and exporter-management behavior. User mode is the intended single-user local policy, but its initial exporter must support runtime-managed credentials.

User-mode files currently live under the container's `/root/.esdiag` and disappear when the container is replaced. The final image currently runs as root, making a cross-platform host bind mount likely to create ownership problems on native Linux.

## Goals / Non-Goals

**Goals:**

- Start the standalone User-mode web service with the generated local Elasticsearch exporter.
- Keep the generated API key outside the encrypted ESDiag keystore.
- Preserve the normal explicit-output-then-environment fallback for web processing.
- Fail instead of silently falling back to stdout when neither the UI nor the environment provides a valid output.
- Persist User-mode hosts, settings, jobs, keystore, and unlock files across container replacement.
- Preserve the existing Processor type-state lifecycle and Exporter implementations.

**Non-Goals:**

- Do not change Service-mode IAP, persistence, or web-feature policy.
- Do not automatically generate or expose a keystore encryption password.
- Do not copy the generated local API key into a saved host or keystore.
- Do not make the current container image non-root or expose container configuration directly as host files in this change.
- Do not change CLI `process` or `collect` output precedence outside `serve` startup.

## Decisions

### Keep `esdiag-local` in explicit User mode

Generated Compose will set `ESDIAG_MODE=user`. User mode provides the local single-user UI and allows saved hosts, jobs, settings, and later exporter changes without requiring IAP headers.

Alternative: use Service mode because it already reads `ESDIAG_OUTPUT_*`. Rejected because its authentication and feature restrictions are incompatible with a browser opened directly on localhost.

### Preserve explicit-output-then-environment fallback

Exporter resolution will use this state transition:

```text
Start
  |
  +-- explicit CLI or UI output ------------> resolve explicit exporter
  |
  +-- no explicit output -------------------> resolve environment exporter
  |                                             |
  |                                             +-- invalid -> startup error
```

This removes the User-mode-only stdout override. Explicit output remains authoritative, while an omitted UI output uses `ESDIAG_OUTPUT_*`; missing or invalid environment configuration fails closed.

Alternative: add a magic `env` output argument to the local Compose command. Rejected because it would encode a local workaround instead of fixing the documented `serve` contract.

### Gate the keystore only for an explicit saved-host selection

The UI already persists an explicit saved output as `Settings.active_target`. Keystore preflight will inspect only that selection. The output control presents an unset selection as `Default`, sends `null`, and deserializes it as `Option::None`; processing then falls through to `ESDIAG_OUTPUT_*` without inspecting saved hosts or inferring ownership by matching URLs.

Selecting a saved secret-backed host continues to require unlock. Clearing or omitting the UI selection returns to environment fallback. No exporter-origin state is required because explicit UI selection is already the boundary between local keystore-backed configuration and runtime configuration.

### Persist User-mode artifacts in a named volume

Generated Compose will mount a dedicated `esdiag-data` named volume at `/root/.esdiag` for the current image. `down` and service-scoped recreation preserve it; `reset --force` removes it through Compose `down --volumes`. Elasticsearch and Kibana volumes remain separate.

The lifecycle coupling check will recognize the new volume. For pre-release state created before this change, existing Elasticsearch and Kibana volumes without `esdiag-data` may add the empty ESDiag volume without invalidating existing credentials, because no prior ESDiag container state was durable.

Alternative: bind `${state_dir}` into the container. Rejected for this release because the image runs as root and can create root-owned host files on native Linux. A future non-root image can reconsider a host-visible bind mount and migration path.

### Keep bootstrap API-key ownership in `esdiag-local`

The generated API key remains in the protected deployment `.env`, is supplied directly to setup and web containers, and is rotated only by the existing local deployment lifecycle. ESDiag treats this as external runtime authentication. The default local output is not serialized into `hosts.yml`, `settings.yml`, or `secrets.yml` merely to make it active.

User-created secure outputs remain separate and use the persistent keystore flow. This avoids two sources of truth and avoids requiring an unattended keystore password.

### Separate setup and browser Kibana URLs

The setup container will retain `http://kibana:5601/s/${ESDIAG_KIBANA_SPACE}` because it imports assets over the Compose network. The ESDiag web container will receive `http://localhost:${ESDIAG_KIBANA_PORT}/s/${ESDIAG_KIBANA_SPACE}` because it embeds that value in links followed by the host browser. The published port remains configurable while the internal service address remains stable.

### Preserve processing type-state transitions

Receiver, Processor, and Exporter type-state transitions do not change. The UI selection layer resolves an explicit exporter or falls through to the existing environment resolver before creating a processing job; the existing Processor receives the resulting `Arc<Exporter>` and follows its current initialized-to-running-to-complete lifecycle.

## Risks / Trade-offs

- **Existing environments containing only an auth variable will now fail instead of using stdout** -> Emit an error naming the missing or invalid `ESDIAG_OUTPUT_*` values without printing secrets.
- **Named volumes are less directly inspectable than host files** -> Keep lifecycle and backup expectations documented; prefer portability and correct ownership for the root-running 0.16 image.
- **An empty ESDiag volume may be added to an existing local deployment** -> Treat absence of this newly introduced volume as a one-time additive migration, while retaining strict coupling for Elasticsearch, Kibana, and credential state.
- **A runtime URL may match a saved secure host** -> Treat only `Settings.active_target` as explicit selection; never infer keystore ownership from URL equality.
- **Container stdout may still contain operational JSON or diagnostics in logs** -> End-to-end tests distinguish tracing output from exported diagnostic documents and verify indexed results in Elasticsearch.

## Migration Plan

1. Remove the User-mode stdout override and make blank UI output use the environment resolver, with unit tests.
2. Limit keystore preflight to an explicit saved-host selection and retain existing settings transitions.
3. Add explicit User mode, browser-reachable Kibana links, and the `esdiag-data` volume to generated standalone Compose.
4. Update lifecycle coupling, reset behavior, documentation, changelog, and standalone tests.
5. As the final runtime test and system primer, submit a synchronous API-key processing job through the web API with `http://elasticsearch:9200` as both the diagnostic source and runtime output. Use the generated local API key, verify the node diagnoses itself into local Elasticsearch without a stdout exporter stream, and confirm real indexing materializes the lazily created mapping fields.
Rollback consists of reverting the implementation commit and recreating the local deployment with the previous image.

## Open Questions

- Running the final image as non-root and migrating the named volume or exposing a host-visible configuration directory remains follow-up work outside this change.
