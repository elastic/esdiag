## Context

`esdiag-local` generates an Elasticsearch API key, persists it in its mode-0600 deployment `.env`, and passes `ESDIAG_OUTPUT_URL` and `ESDIAG_OUTPUT_APIKEY` to both setup and web containers. The web container runs `esdiag serve` without an explicit mode or output. Mode therefore defaults to User, while `resolve_serve_exporter` deliberately chooses `Exporter::default()` for User mode and ignores the environment. Because the default exporter is `Stream`, processed documents go to container stdout instead of Elasticsearch.

Switching the local container to Service mode is not viable: Service mode requires identity-aware-proxy headers and removes local host, saved-job, settings, and exporter-management behavior. User mode is the intended single-user local policy, but its initial exporter must support runtime-managed credentials.

User-mode files currently live under the container's `/root/.esdiag` and disappear when the container is replaced. The final image currently runs as root, making a cross-platform host bind mount likely to create ownership problems on native Linux.

## Goals / Non-Goals

**Goals:**

- Start the standalone User-mode web service with the generated local Elasticsearch exporter.
- Keep the generated API key outside the encrypted ESDiag keystore.
- Distinguish runtime-backed credentials from local keystore-backed credentials.
- Fail startup instead of silently falling back to stdout when runtime output configuration is incomplete or invalid.
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

### Resolve `serve` output independently from runtime mode

Initial exporter resolution will use this state transition:

```text
Start
  |
  +-- explicit output ----------------------> resolve explicit exporter
  |
  +-- any ESDIAG_OUTPUT_* variable present -> resolve environment exporter
  |                                             |
  |                                             +-- invalid -> startup error
  |
  +-- User mode ----------------------------> Stream exporter
  |
  +-- Service mode -------------------------> startup error
```

This restores the documented environment-output behavior while retaining stdout as an intentional fallback only for an entirely unconfigured User-mode server. Explicit output remains authoritative.

Alternative: add a magic `env` output argument to the local Compose command. Rejected because it would encode a local workaround instead of fixing the documented `serve` contract.

### Track exporter credential origin explicitly

Server state will carry an origin alongside the active exporter, with at least:

```text
RuntimeConfigured
LocalKeystore
UnsecuredOrStream
```

Startup from `ESDIAG_OUTPUT_*` produces `RuntimeConfigured`. Selecting a saved host that requires a secret produces `LocalKeystore`. Selecting stdout, files, directories, or no-auth outputs produces `UnsecuredOrStream`. Exporter updates commit the exporter and origin together only after validation succeeds.

Keystore preflight will consult origin first. A runtime-configured exporter is already authenticated and bypasses keystore bootstrap even when its URL matches a saved secure host. A locally selected secure host continues to require unlock.

The `Exporter` trait boundary and concrete Elasticsearch exporter remain unchanged; origin is orchestration metadata owned by web server state rather than an exporter transport concern.

Alternative: infer origin from URL and saved-host records. Rejected because identical URLs can be backed by different credential sources and caused the current false keystore dependency.

### Persist User-mode artifacts in a named volume

Generated Compose will mount a dedicated `esdiag-data` named volume at `/root/.esdiag` for the current image. `down` and service-scoped recreation preserve it; `reset --force` removes it through Compose `down --volumes`. Elasticsearch and Kibana volumes remain separate.

The lifecycle coupling check will recognize the new volume. For pre-release state created before this change, existing Elasticsearch and Kibana volumes without `esdiag-data` may add the empty ESDiag volume without invalidating existing credentials, because no prior ESDiag container state was durable.

Alternative: bind `${state_dir}` into the container. Rejected for this release because the image runs as root and can create root-owned host files on native Linux. A future non-root image can reconsider a host-visible bind mount and migration path.

### Keep bootstrap API-key ownership in `esdiag-local`

The generated API key remains in the protected deployment `.env`, is supplied directly to setup and web containers, and is rotated only by the existing local deployment lifecycle. ESDiag treats this as external runtime authentication. The default local output is not serialized into `hosts.yml`, `settings.yml`, or `secrets.yml` merely to make it active.

User-created secure outputs remain separate and use the persistent keystore flow. This avoids two sources of truth and avoids requiring an unattended keystore password.

### Preserve processing type-state transitions

Receiver, Processor, and Exporter type-state transitions do not change. The server selects and validates the exporter plus credential origin before creating a processing job; the existing Processor receives the resulting `Arc<Exporter>` and follows its current initialized-to-running-to-complete lifecycle. Invalid runtime output fails before a server or Processor is started.

## Risks / Trade-offs

- **Existing environments containing only an auth variable will now fail instead of using stdout** -> Emit an error naming the missing or invalid `ESDIAG_OUTPUT_*` values without printing secrets.
- **Named volumes are less directly inspectable than host files** -> Keep lifecycle and backup expectations documented; prefer portability and correct ownership for the root-running 0.16 image.
- **An empty ESDiag volume may be added to an existing local deployment** -> Treat absence of this newly introduced volume as a one-time additive migration, while retaining strict coupling for Elasticsearch, Kibana, and credential state.
- **Exporter and origin could diverge during settings updates** -> Validate both as one candidate and update both under the same server-state write operation only after successful exporter construction.
- **Container stdout may still contain operational JSON or diagnostics in logs** -> End-to-end tests distinguish tracing output from exported diagnostic documents and verify indexed results in Elasticsearch.

## Migration Plan

1. Add exporter-origin state and environment-aware `serve` resolution with unit tests.
2. Update keystore preflight and settings transitions to use origin.
3. Add explicit User mode and the `esdiag-data` volume to generated standalone Compose.
4. Update lifecycle coupling, reset behavior, documentation, changelog, and standalone tests.
5. Run a live local-stack processing test that verifies indexed documents and no stdout exporter stream.
6. Rebuild and push all 0.16 image aliases, update the curated draft notes, move the unpublished release tag with approval, and rerun draft asset verification.

Rollback before publication consists of reverting the release-branch commits and rebuilding the unpublished image aliases. Do not move a published tag.

## Open Questions

- Running the final image as non-root and migrating the named volume or exposing a host-visible configuration directory remains follow-up work outside this change.
