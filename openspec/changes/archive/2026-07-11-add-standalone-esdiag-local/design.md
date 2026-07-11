## Context

`bin/esdiag-control` currently combines two concerns:

- repository development and container image construction; and
- end-user lifecycle management for a local ESDiag-configured Elastic Stack.

The lifecycle path depends on `Cargo.toml`, `example.env`, `docker/Dockerfile`, and the Compose definitions under `docker/`. Its `up` command builds an image even when a published ESDiag image is available. This prevents the script from being distributed independently.

Elastic's `start-local` script establishes a useful containment precedent: one downloaded script can generate durable environment and Compose state before starting official containers. ESDiag additionally needs a repeatable command interface and must run `esdiag setup` successfully before the deployment is considered ready.

## Goals / Non-Goals

**Goals:**
- Distribute one executable `esdiag-local` script that needs no repository checkout or separately downloaded configuration assets.
- Pull exact, compatible Elasticsearch, Kibana, and ESDiag image versions from `docker.elastic.co` by default.
- Produce a secure, idempotent, fully configured local deployment.
- Preserve source-built ESDiag images and repository-oriented workflows through `esdiag-control`.
- Share one lifecycle implementation between official-image and custom-build workflows.
- Support Docker Compose v2 and Podman Compose with explicit validation and integration coverage.
- Allow the standalone script to check the official ESDiag repository for a newer release and safely replace itself.

**Non-Goals:**
- Installing Docker, Podman, or a Compose provider.
- Providing a production deployment topology.
- Adding an `esdiag-control smoke` command.
- Replacing Compose with imperative container/network/volume management.
- Supporting arbitrary remote Elasticsearch or Kibana targets through `esdiag-local`.
- Changing Rust CLI, processing, setup, or web-server behavior.

## Decisions

### 1) Split distribution and development entry points

Decision:
- `bin/esdiag-local` is the canonical implementation of local stack lifecycle behavior and the only script published as the standalone local-stack artifact.
- `bin/esdiag-control` remains repository-dependent and retains `build` and `buildx` for development and audited source builds.
- Existing repository lifecycle commands remain available, but delegate to `bin/esdiag-local` with `ESDIAG_IMAGE_TAG` set to the locally built image and pulling disabled.
- `esdiag-control` uses `${repository_root}/target/esdiag-local` as its default delegated state directory, separate from standalone state and volumes. An explicit caller-supplied state directory still takes precedence.
- `esdiag-control up` preserves the existing build-to-test workflow by building or selecting `esdiag:${version}` and invoking the equivalent of `ESDIAG_LOCAL_DIR="${repository_root}/target/esdiag-local" ESDIAG_IMAGE_TAG="esdiag:${version}" bin/esdiag-local up --pull never`.

Rationale:
- The public script has a coherent, repository-independent contract.
- Security-sensitive users can audit and build from source without maintaining a separate startup implementation.
- Delegation prevents startup, setup, and teardown behavior from drifting between the two scripts.

### 2) Embed templates and generate durable runtime state

Decision:
- `esdiag-local` contains quoted heredoc templates for its Compose definition and initial environment configuration.
- It writes generated files beneath `${ESDIAG_LOCAL_DIR:-$HOME/.esdiag/local}` unless `--state-dir` overrides the location.
- The state directory contains at least `.env`, `compose.yml`, and failure logs. It is created with restrictive permissions, and `.env` is mode `0600`.
- Commands resolve state independently of the script location and current working directory.
- The script never `source`s or `eval`s `.env`. It reads only documented keys with an allowlisted parser and writes updates through a restrictive temporary file followed by atomic rename.
- Host ports are stored in `.env` as `ESDIAG_ELASTICSEARCH_PORT`, `ESDIAG_KIBANA_PORT`, and `ESDIAG_PORT`, defaulting to `9200`, `5601`, and `2501`.

Rationale:
- Generated files remain inspectable and work naturally with Compose while preserving the one-download requirement.
- A self-extracting archive adds opacity and extraction dependencies without benefit for two small text templates.
- Parsing configuration as data prevents a modified `.env` from executing arbitrary shell commands.

### 2a) Target Bash with platform-aware execution

Decision:
- `esdiag-local` targets Bash 3.2 or newer on Linux and macOS using an `#!/usr/bin/env bash` entry point.
- The script avoids Bash features introduced after 3.2 unless startup detects a newer required version.
- Operating-system adapters handle executable path resolution, SHA-256 tooling, filesystem metadata, resource inspection, browser launch, and clipboard integration without assuming GNU userland on macOS.
- Linux context detection includes WSL-specific browser and clipboard behavior where the corresponding Windows bridge commands are available.

Rationale:
- Bash matches the existing control script while a Bash 3.2 baseline supports the system Bash shipped by macOS.
- Central adapters keep macOS/Linux differences out of lifecycle logic and make them independently testable.

### 3) Pin release versions and pull official images

Decision:
- Every released script embeds an ESDiag version and a tested, compatible Elastic Stack version.
- Default images use exact tags:
  - `docker.elastic.co/esdiag/esdiag:${ESDIAG_VERSION}`
  - `docker.elastic.co/elasticsearch/elasticsearch:${ELASTIC_VERSION}`
  - `docker.elastic.co/kibana/kibana:${ELASTIC_VERSION}`
- `latest` is never the release default.
- `up` pulls the pinned images unless an explicit image override and pull policy say otherwise.
- ESDiag image selection follows explicit precedence: a command-line image option, then `ESDIAG_IMAGE_TAG`, then the image recorded in existing state or the embedded official-image default.
- `ESDIAG_IMAGE_TAG` applies to the ESDiag service and one-shot setup container so both execute the same overridden image.
- Release automation verifies that the matching multi-platform ESDiag image is available before publishing the script.
- Existing state remains pinned when a newer script is run against it; version changes require `up --upgrade` or explicit version overrides.
- After a successful script self-update, the updater reports that stack versions remain unchanged and directs the user to `esdiag-local up --upgrade`.
- `up --upgrade` stages configuration using the running script's embedded versions, pulls and validates the new images, reruns setup, and commits new version state only after the upgraded deployment reaches `Ready`. Failure leaves the prior version state available for a normal `up` recovery.

Rationale:
- Script and embedded ESDiag assets remain version-compatible and reproducible.
- Explicit upgrades prevent an updated script from silently changing a persistent local cluster.

### 3a) Publish predictable GitHub release assets

Decision:
- `https://ela.st/esdiag-local` is the stable human-facing discovery URL and redirects to `https://github.com/elastic/esdiag/releases/latest`.
- Every tagged ESDiag release attaches an executable asset named `esdiag-local` whose embedded version and image defaults are pinned to that release.
- The same release attaches its checksum as `esdiag-local.sha256`.
- Release automation creates and attaches both assets after the matching ESDiag image manifest validation succeeds.
- Direct automation uses GitHub's predictable latest-release asset URLs:
  - `https://github.com/elastic/esdiag/releases/latest/download/esdiag-local`
  - `https://github.com/elastic/esdiag/releases/latest/download/esdiag-local.sha256`

Rationale:
- The short URL gives users a durable release-discovery entry point.
- Stable asset names allow documentation and the self-updater to locate the latest version without embedding a version in the download URL, while the script contents remain version-pinned.

### 4) Use staged, fail-closed startup

Decision:
- `up` performs these state transitions:
  - `Validated` after runtime, Compose, architecture, port, disk, and memory checks.
  - `Configured` after durable configuration and credentials exist.
  - `InfrastructureReady` after Elasticsearch and Kibana are healthy.
  - `CredentialsReady` after an ESDiag API key exists.
  - `AssetsReady` after a one-shot ESDiag container completes `esdiag setup` against both Elasticsearch and Kibana.
  - `Ready` after the ESDiag web container is healthy and all public endpoints pass verification.
- The web container is not started with an empty credential and is not reported ready before asset setup completes.
- Each readiness wait has a bounded timeout. Failures retain state and capture relevant Compose/container logs without deleting persistent data.

Rationale:
- A running container is not equivalent to a configured ESDiag deployment.
- Staging removes the current web-container restart and makes partial failures recoverable through `setup` or a repeated `up`.

### 5) Define distribution-oriented lifecycle commands

Decision:
- `esdiag-local` provides `up`, `down`, `status`, `logs`, `setup`, `auth`, `secrets`, `reset`, `update`, `help`, and `version`.
- `up` is idempotent and reconciles generated configuration without rotating valid credentials or deleting data.
- `down` removes containers and the project network while retaining credentials, generated configuration, and volumes.
- `reset` removes containers, volumes, and generated state only after explicit confirmation; non-interactive use requires `--force`.
- `setup` reruns the one-shot asset installation for recovery.
- Port changes are made by editing `.env`; command-line port flags are not provided. Startup validates that all configured ports are numeric, in range, distinct, and available before container creation.

Rationale:
- Keeping lifecycle actions in the downloaded artifact allows it to remain useful after first installation.
- Destructive behavior is separated from routine shutdown.

### 6) Preserve local security boundaries

Decision:
- Security is always enabled because agent assets and other stack features require it.
- Host ports bind to `127.0.0.1` by default.
- Elasticsearch and Kibana use separate named volumes.
- Generated passwords and encryption keys use cryptographically random host facilities and are persisted with restrictive permissions.
- API keys and passwords are not written to normal or debug logs. Credentials are displayed only through intentional first-run or credential-reporting output.
- One generated Elasticsearch API key is persisted in `.env` and shared by the one-shot setup container and ESDiag service container.
- Credentials and named volumes are treated as one deployment state. The script does not attempt automatic credential recovery: missing `.env` with existing volumes, or missing volumes with initialized credential state, fails with guidance to restore state or run a confirmed `reset` for a new deployment.

Rationale:
- A convenience deployment should not expose unauthenticated services to the local network or leak durable credentials.
- Avoiding credential recovery prevents the script from silently replacing access to an existing cluster or presenting stale credentials as valid.

### 6a) Expose only intentional, script-friendly secrets

Decision:
- `esdiag-local secrets password` writes only the generated `elastic` user password to standard output.
- `esdiag-local secrets apikey` writes only the generated Elasticsearch API key used by ESDiag to standard output.
- Successful secret output contains one raw value with no label, color, timestamp, or log prefix, making command substitution and pipelines reliable. Diagnostics and errors go to standard error.
- `status`, `auth`, `logs`, debug output, and general help never include secret values.
- If security is disabled, state is missing, or the requested credential has not been generated, the command exits non-zero without emitting a value on standard output.
- Help output detects the host platform and available clipboard utilities and shows an appropriate example, including `esdiag-local secrets password | pbcopy` on macOS, `wl-copy`/`xclip`/`xsel` on Linux, or `clip.exe` under WSL.
- When secure `up` is about to launch a browser, it makes a best-effort attempt to pipe the Elastic password to the detected clipboard utility immediately before browser launch. It reports success without printing the password; clipboard failure is non-fatal and produces manual platform-specific guidance.
- Automatic password copying is disabled with `--copy-password=false` and does not occur when browser launch is disabled, security is disabled, or no supported clipboard utility is available.
- The API key is never copied automatically because it is not needed for Kibana login.

Rationale:
- Raw standard output supports `$(esdiag-local secrets password)` and direct piping without requiring users to parse `.env`.
- A narrowly scoped subcommand keeps accidental secret disclosure out of ordinary operational output.
- Copying the password immediately before opening Kibana removes a common first-run interruption while retaining an opt-out for users with stricter clipboard policies.

### 7) Test standalone and delegated workflows separately

Decision:
- Add `tests/esdiag-local.sh` for shellcheck, help/version behavior, execution outside the repository, secure template generation and startup, setup completion, idempotent repeated `up`, status/auth, down, and destructive reset behavior.
- Update `tests/bin/esdiag-control.sh` to verify local image construction and delegation with `ESDIAG_IMAGE_TAG=esdiag:${version}` and pulling disabled.
- Runtime integration tests use isolated state directories and cover both Docker Compose v2 and Podman Compose where CI runners are available.
- Update tests copy the script to a temporary writable installation directory and inject a fixture-backed fake `curl` and checksum tools through `PATH`; they assert requests use the hard-coded official URLs and never replace the repository script.

Rationale:
- Standalone containment can regress without container behavior changing, while delegation can regress without standalone behavior changing. Both boundaries need direct tests.

### 8) Provide an explicit, verified self-update workflow

Decision:
- `esdiag-local update --check` checks the latest official release at `https://github.com/elastic/esdiag` and reports whether a newer standalone artifact is available without changing local files.
- `esdiag-local update` performs the same check and, when a newer release exists, downloads the release's `esdiag-local` and `esdiag-local.sha256` assets with `curl`.
- The updater verifies the checksum, validates that the downloaded artifact is a syntactically valid script reporting the expected version, applies executable permissions, and atomically renames it over the resolved path of the running script.
- Updates are opt-in. Lifecycle commands such as `up` do not contact GitHub or replace the executable automatically.
- The update source is fixed to the official `github.com/elastic/esdiag` repository in release builds. Image registry overrides do not change the script update source.
- If the script or its parent directory is not writable, the command exits without mutation and prints the exact official artifact URL for manual installation.
- Self-update requires an executed regular file. The script resolves a PATH invocation to an absolute path, safely handles spaces, refuses automatic replacement when invoked through a symbolic link or when sourced, and downloads the candidate into the same directory before atomic rename.

Rationale:
- A standalone artifact needs a repository-independent path to receive fixes after installation.
- An explicit command avoids unexpected executable changes during stack lifecycle operations.
- Checksum verification and atomic replacement prevent truncated, tampered, or invalid downloads from replacing a working installation.

### 9) Detect and adapt to the container runtime

Decision:
- `esdiag-local` continues the `esdiag-control` detection pattern: use Podman when available, otherwise Docker, with an explicit runtime override for deterministic automation.
- Detection verifies that the selected engine is reachable and that a compatible Compose provider is available before modifying state.
- Provider adapters select supported Compose syntax for health waiting and no-pull execution instead of assuming Docker and Podman accept identical flags.

Rationale:
- Retaining automatic detection preserves the existing local workflow while capability validation produces earlier, actionable failures.

## Risks / Trade-offs

- [Embedded Compose templates duplicate declarative YAML inside shell] -> Keep `esdiag-local` canonical, generate the YAML deterministically, and test the generated file with Compose configuration validation.
- [Docker and Podman Compose semantics differ] -> Detect the selected provider, validate required capabilities, and maintain a provider-specific integration matrix.
- [Release script references an unavailable image] -> Gate artifact publication on registry manifest availability for supported architectures.
- [Existing `esdiag-control` users see state-location changes] -> Preserve command names, document delegation, and allow an explicit state directory during migration.
- [Partial startup leaves containers running] -> Retain diagnosable state, print the recovery command, and make repeated `up` safe rather than destructively rolling back data.
- [User-edited generated files drift from the embedded schema] -> Record the schema/script version and refuse incompatible automatic regeneration without `--upgrade`.
- [Self-update is interrupted or receives invalid content] -> Download beside the installed script, verify checksum and script identity, and use atomic rename only after every validation succeeds.
- [GitHub is unavailable or rate-limits checks] -> Treat update-check failure as non-destructive and leave the current executable untouched; ordinary lifecycle commands remain independent of GitHub.
- [Clipboard contents can be read by other local applications] -> Copy only for an imminent interactive Kibana login, clearly report the action, never auto-copy API keys, and provide `--copy-password=false`.
- [Script update and stack upgrade are conflated] -> Keep them separate and print the explicit `up --upgrade` next step after script replacement.
- [Checksum and artifact share one trust domain] -> Document that SHA-256 verifies download integrity, not independent authenticity; HTTPS and GitHub repository controls remain the trust boundary.

## Migration Plan

- Add and test `bin/esdiag-local` while retaining existing `esdiag-control` command names.
- Move local Compose and environment generation into `esdiag-local`.
- Change `esdiag-control up`, `down`, `setup`, and `auth` to invoke `bin/esdiag-local` with repository-derived image options and isolated `target/esdiag-local` state.
- Update quick-start documentation to use the downloaded `esdiag-local` artifact and move source-build guidance to `esdiag-control` documentation.
- Have the release workflow create or retain a draft release, verify the versioned ESDiag image manifests, render and validate a non-SNAPSHOT script, attach and verify `esdiag-local` and `esdiag-local.sha256`, and publish the release only after every gate succeeds.
- Keep rollback possible by restoring the prior `esdiag-control` lifecycle implementation; generated local data remains in named volumes and is not automatically removed.
