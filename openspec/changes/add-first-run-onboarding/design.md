## Context

Local ESDiag state is currently split across `hosts.yml`, `secrets.yml`, `jobs.yml`, unlock state, and a two-field `settings.yml` used by desktop/user-mode server startup. There is no CLI first-run command or general persistent preference model. The Agent Skill consequently tries to detect and assemble missing state itself, which puts credential handling and CLI onboarding in prompt instructions.

The existing domain model already separates concerns correctly. `KnownHost` stores non-secret endpoint metadata and a secret reference, the keystore owns authentication material, `SavedJobs` references known hosts, a send-role Elasticsearch host can identify its view-role Kibana host through `viewer`, and `Identifiers` already honors an explicit `--user` and `ESDIAG_USER`. Initialization should compose those models, not replace them.

The runtime environment also already represents one output deployment: `ESDIAG_OUTPUT_URL` and `ESDIAG_OUTPUT_*` authentication identify Elasticsearch, while `ESDIAG_KIBANA_URL` identifies the Kibana instance attached to that Elasticsearch cluster and uses the same authentication. This relationship must remain atomic so commands cannot accidentally combine Elasticsearch from one deployment with Kibana or credentials from another.

## Goals / Non-Goals

**Goals:**

- Give a newly installed CLI user one secure, terminal-guided path to a successful repeatable workflow.
- Persist only non-secret preferences and references in one general `esdiag.yml` file.
- Reuse `KnownHost`, viewer links, keystore secrets, saved jobs, and existing clients in process.
- Resolve one canonical output deployment consistently across CLI commands while exposing a reusable backend resolver for future GUI and Agent Builder consumers.
- Default diagnostic user identifiers from persisted configuration after explicit CLI and environment values.
- Make initialization resumable, idempotent, and safe around existing files.
- Keep agents out of secret collection by making credential entry a terminal-native interaction.

**Non-Goals:**

- Storing URLs, API keys, passwords, or complete host/job definitions in `esdiag.yml`.
- Replacing `hosts.yml`, `secrets.yml`, or `jobs.yml`.
- Persisting user configuration in service mode.
- Automatically provisioning licenses, inference services, connectors, or third-party model credentials.
- Automatically running `esdiag setup` without explicit user approval, except
  for the lifecycle-owned assets implied by approval to create a new local core
  stack.
- Providing a non-interactive bootstrap language in the first iteration; existing commands and environment variables remain available for automation.
- Performing Agent Builder conversations or diagnostic analysis.
- Downloading agent skills or requiring any supported coding agent to be installed.
- Providing a web or desktop onboarding flow.
- Changing web/desktop user-mode startup, settings persistence, or service-mode behavior.
- Migrating or removing the legacy desktop `settings.yml`; the GUI onboarding follow-on owns that transition.
- Installing or managing coding-agent skills; the agent CLI change owns that command surface and behavior.

## Decisions

### Use one application configuration file containing references

Introduce a versioned `ApplicationConfig` stored at `~/.esdiag/esdiag.yml`:

```yaml
version: 1
user: reno@example.com
output:
  default: diagnostic-output
  authenticated_on: "2026-08-12T20:00:00Z"
  assets_version: "0.17.0-SNAPSHOT"
job:
  default: production-standard
```

`output.default` names a saved Elasticsearch host with role `send`; that host's `viewer` names the saved Kibana host with role `view`. `output.authenticated_on` records successful endpoint authentication, and `output.assets_version` records the ESDiag asset version after setup. `job.default` names an entry in `jobs.yml`. Authentication remains a secret reference in each host and encrypted material remains in `secrets.yml`. The output Elasticsearch and Kibana hosts may share one secret identifier when they use the same credentials.

This makes configuration a small preference layer rather than a second inventory. Alternative considered: store both URLs and credentials in `esdiag.yml`. That duplicates `KnownHost`, creates another secret surface, and allows configuration to drift.

### Defer desktop settings migration to GUI onboarding

This change adds `esdiag.yml` for CLI preferences without changing how existing web/desktop user mode reads or writes `settings.yml`. `esdiag init` does not delete, rewrite, or claim to migrate the desktop settings file. This temporary coexistence is an explicit sequencing boundary, not a permanent two-configuration design.

The follow-on GUI onboarding change will use the same `ApplicationConfig` and `OutputDeployment` services, define how representable `active_target` and `kibana_url` values migrate, preserve a backup, and remove the legacy write path only after user-mode compatibility is covered. Until then, CLI output selection comes from explicit arguments, a complete environment deployment, or `esdiag.yml`; it does not infer preferences from `settings.yml`.

Alternative considered: migrate desktop settings as part of CLI onboarding. That would change UI startup and persistence behavior without specifying the GUI onboarding experience, preventing the two flows from being reviewed and delivered independently.

### Resolve the output deployment as one atomic value

Add a shared `OutputDeployment` resolver that yields an Elasticsearch host, its Kibana viewer when required, one resolved authentication method, and space-normalized Kibana routing. It uses this precedence:

1. An explicit command target.
2. A complete runtime environment deployment beginning with `ESDIAG_OUTPUT_URL`, with `ESDIAG_KIBANA_URL` required by operations that need Kibana.
3. The `ApplicationConfig.output.default` saved-host reference.
4. A configuration error.

Once a source wins, all endpoints and credentials come from that source. A partial environment deployment fails closed; it is never completed with a saved host or preference from another deployment. `ESDIAG_OUTPUT_APIKEY` or the existing output username/password pair authenticates both environment-backed Elasticsearch and Kibana. No analysis-specific Elasticsearch URL or Kibana credential variables are introduced.

For saved deployments, the send host and its viewer may reference the same keystore secret. The resolver uses existing `KnownHost` and keystore APIs and never copies decrypted material into configuration or outcomes.

### Make initialization a resumable state machine

Model the wizard as explicit stages:

```text
InspectExisting
    -> Identity
    -> Keystore
    -> OutputDeployment
    -> CollectionHosts
    -> DefaultJob
    -> Complete
```

Each transition validates its resulting domain object before persisting it. Completed stages are detected on the next run and offered for reuse or explicit replacement. Files are written atomically where their existing APIs support it; initialization never shells out to another `esdiag` process. Stage services accept typed inputs and return typed results without terminal prompting so the follow-on GUI can compose the same backend operations.

The wizard persists independently valid stages rather than attempting a transaction across four files. An interruption can therefore leave a valid keystore or host without falsely marking initialization complete. `ApplicationConfig` fields are written as their corresponding validated stages complete, but file existence alone is not a completion marker: readiness is derived from its required references and the referenced domain state. This keeps completion semantics independent of whether the CLI or a future GUI writes the shared configuration.

### Keep credential entry on the controlling terminal

Keystore passwords, API keys, and host passwords use hidden TTY prompts and existing keystore functions. They never appear in command arguments generated by the wizard, structured outcomes, logs, `esdiag.yml`, or onboarding documentation examples. When no controlling terminal is available and a required secret is not already supplied through an existing secure mechanism, `init` exits with actionable guidance rather than reading secrets from ordinary stdin.

This boundary lets an Agent Skill recommend `esdiag init` without becoming a credential broker.

### Configure an output deployment as linked existing host types

The output step creates or selects:

- One Elasticsearch host with role `send`.
- One Kibana host with role `view`.
- A `viewer` reference from the send host to the view host.
- One shared secret reference by default, with distinct existing secrets allowed when explicitly selected.

Both clients are tested before the configuration becomes active. Initialization
checks whether ESDiag assets required for processing and Agent Builder use are
available. For an existing local or remote deployment, the wizard offers to run
the existing setup operation and explains the privilege/license implications;
declining preserves the configured endpoints and reports setup as incomplete.

Starting a new local core stack is different: the user's approval to provision
that stack also approves its required local ESDiag assets. The Rust
local-stack lifecycle installs those assets as part of reaching a ready local
deployment, so `init` records the asset version without asking the same
question again. The standalone full-stack path likewise owns setup during
stack startup; `esdiag-local exec -- init` configures only container-owned
user state.

Alternative considered: add a new deployment inventory type. The existing host-role and viewer relationship already models the pair and is consumed by exporter link generation, so a parallel type would add migration and synchronization costs.

### Make collection hosts and the first job part of success

After the output deployment is usable, initialization creates or selects at least one collect-role host, offers a loop for additional hosts, and builds the first saved job through the existing typed job model. The default successful path creates a processing job whose output is the configured send host, so running it produces indexed data rather than only a local archive. The user may explicitly select a collect-only job instead.

The selected job name is stored as `job.default`; the job body remains solely in `jobs.yml`. Initialization may finish without additional hosts beyond the first, but it does not claim a repeatable diagnostic workflow is ready without a valid default job.

### Apply identifier precedence explicitly

Default user resolution becomes:

```text
--user > ESDIAG_USER > ApplicationConfig.user > absent
```

The configured user is copied into the normal `Identifiers` value at workflow construction time. No global mutable environment value is synthesized.

## Risks / Trade-offs

- **Cross-file interruption leaves partial state** → Persist only independently valid stages, detect them on rerun, and write the completion configuration last.
- **Environment and saved configuration could identify different deployments** → Select one complete source atomically and fail on partial environment configuration.
- **Initialization becomes a large interactive workflow** → Keep each stage backed by existing domain APIs and make the stage boundaries independently testable.
- **Setup may require elevated cluster privileges or a license** → Validate first, ask explicitly before setup, and distinguish configured endpoints from provisioned assets.
- **A shared API key might not be valid for both products** → Test both clients and allow explicitly selected distinct existing secret references while keeping shared credentials the default.
- **`esdiag.yml` and legacy desktop `settings.yml` temporarily coexist** → Keep their consumers isolated in this change and make consolidation an explicit requirement of the GUI onboarding follow-on.

## Migration Plan

1. Add `ApplicationConfig`, path resolution, atomic serialization, and validation without changing web/desktop consumers.
2. Add atomic `OutputDeployment` resolution and migrate omitted-output CLI consumers.
3. Add persisted default-user resolution to identifier construction.
4. Extract flow-neutral backend operations for configuration, credentials, hosts, jobs, endpoint validation, setup, and readiness.
5. Implement and test the terminal-specific staged `esdiag init` orchestration over those operations.
6. Update CLI and configuration documentation and the changelog.
7. Leave web/desktop settings behavior unchanged for the GUI onboarding follow-on.

Rollback leaves `settings.yml`, `hosts.yml`, `secrets.yml`, and `jobs.yml` unchanged. `esdiag.yml` contains no secrets and can be ignored safely by older releases.

## Open Questions

None. Asset setup is an explicit offer for existing and remote deployments;
approval to create a new local core stack implies its lifecycle-owned assets.
The first version is intentionally interactive.
