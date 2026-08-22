## Why

ESDiag persists hosts, secrets, and jobs, but a newly installed CLI user must discover and assemble those pieces manually. A terminal-native first-run workflow and one general application configuration file can establish a secure, repeatable diagnostic workflow without moving credential entry or setup orchestration into an Agent Skill.

## What Changes

- Add an interactive `esdiag init` workflow that can be resumed safely and never overwrites valid existing state without confirmation.
- Capture a default user name or email for diagnostic `Identifiers` when `--user` and `ESDIAG_USER` are absent.
- Create or unlock the encrypted keystore through terminal-native secret prompts that do not expose credentials to an agent conversation or structured output.
- Configure and validate the diagnostic output deployment as one Elasticsearch send host linked to one Kibana view host, sharing one keystore secret when their authentication is the same.
- Treat user approval to provision a new local core stack as approval for that
  lifecycle's required ESDiag assets; retain a separate asset-installation
  confirmation for existing local and remote output deployments.
- Create the first collect host, optionally add more collect hosts, and configure the first saved collection or processing job.
- Introduce `~/.esdiag/esdiag.yml` as the general non-secret application configuration containing preference values and references such as the default user, output host, and saved job.
- Keep `ESDIAG_OUTPUT_*` and `ESDIAG_KIBANA_URL` as runtime/deployment overrides for the same output deployment, with shared output credentials, rather than creating analysis-specific URL or credential variables.
- Expose the configuration, deployment, credential, host, job, validation, and setup operations as flow-neutral backend services that a follow-on GUI onboarding flow can reuse.

## Capabilities

### New Capabilities

- `cli-first-run-onboarding`: Interactive, resumable initialization of identity, keystore, output deployment, collection hosts, and the first saved job.
- `application-configuration`: General non-secret configuration persistence, migration, precedence, validation, and canonical output-deployment resolution.

### Modified Capabilities

- `collection-identifiers`: Use the persisted default user when an invocation does not provide a user through CLI or environment configuration.

## Impact

- **Target Elastic products:** Initialization configures Elasticsearch as the processed-diagnostic destination and its attached Kibana instance; source hosts may be Elasticsearch, Kibana, or Logstash according to existing collection support.
- **CLI:** Adds `esdiag init` and a shared configuration resolver used by normal CLI commands. Interactive prompts remain human-oriented, while the final initialization result follows the standard structured CLI outcome contract when that change is available.
- **Local state:** Adds `~/.esdiag/esdiag.yml` and composes existing `hosts.yml`, `secrets.yml`, and `jobs.yml` rather than duplicating their contents. Existing `settings.yml` behavior is unchanged until the GUI onboarding follow-on.
- **Web UI/Desktop:** No onboarding or settings-flow behavior changes in this change. A follow-on GUI change will reuse the same backend services and define user-mode migration; service mode remains environment-driven.
- **Core processing:** Identifier defaulting and omitted-output resolution gain configuration fallbacks; collection and processing behavior otherwise remains unchanged.
- **Agent CLI:** Agent skill installation and its command surface are out of scope and will be delivered by the agent CLI change.
