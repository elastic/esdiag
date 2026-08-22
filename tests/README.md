# Test Utilities

This directory contains integration tests and opt-in test utilities for workflows
that need local services or externally managed Elastic Stack instances.

## ESDiag Lite Shell Tests

`tests/bin/esdiag-lite.sh` validates the portable `bin/esdiag-lite.sh` helper,
including Bash 3.2 version predicates, generated version-aware requests,
environment authentication, API-key precedence, archive behavior, and the
`--archive=none` path. It uses local mock `curl` and `zip` executables; no
Elasticsearch cluster is required.

Run it from the repository root:

```sh
bash tests/bin/esdiag-lite.sh
```

`tests/bin/esdiag-lite.ps1` validates the Windows PowerShell collector using the
built-in PowerShell parser and mocked web/archive commands. Run it on Windows
PowerShell 5.1 or newer:

```powershell
powershell -NoProfile -File tests/bin/esdiag-lite.ps1
```

## esdiag-control Integration Script

`tests/bin/esdiag-control.sh` is an opt-in shell integration suite for the
user-facing `bin/esdiag-control` helper. It exercises the control script itself:

- runs `shellcheck` against `bin/esdiag-control`
- verifies help output
- builds the local `esdiag` container image
- starts and authenticates the security-enabled local stack
- runs setup against the started stack
- tears the stack down

Run it from the repository root:

```sh
./tests/bin/esdiag-control.sh
```

To run one test function:

```sh
./tests/bin/esdiag-control.sh --only command_help_prints_usage
```

The script requires `shellcheck`, `curl`, `jq`, `grep`, `sed`, and either
`podman` or `docker`. It writes its combined command log to
`target/test-esdiag-control.log` and uses a temporary `.env.test` file copied
from `.env` or `example.env`.

## CLI End-To-End Suite

`tests/bin/esdiag-cli-e2e.sh` is an opt-in release smoke suite for a Linux host with
container runtime support. It exercises the full local workflow against the
`esdiag-control` Elastic Stack:

- starts the full-stack local environment with `./bin/esdiag-control up`
- installs the current checkout with `cargo install --path`
- creates an isolated keystore and saved hosts for local Elasticsearch and Kibana
- collects from Elasticsearch and Kibana
- processes Elasticsearch known-host diagnostics into Elasticsearch
- saves and runs a compound `collect -> process -> send` job named `test-job`
- validates `metrics-diagnostic-esdiag` contains diagnostic report documents for
  the processing runs

Run it from the repository root:

```sh
./tests/bin/esdiag-cli-e2e.sh
```

The script requires `cargo`, `curl`, `jq`, and either `podman` or `docker`.
The suite isolates CLI state under `target/pre-release-e2e/<run-id>/home` by
setting `HOME`, `ESDIAG_HOSTS`, and `ESDIAG_KEYSTORE` for every installed
`esdiag` command. It also passes `-b false` to `esdiag-control up` so headless
Linux runs do not fail trying to launch a browser. Logs are written to
`target/pre-release-e2e/<run-id>/logs`.

Useful overrides:

```sh
ESDIAG_E2E_ENV_FILE=.env.ironhide ./tests/bin/esdiag-cli-e2e.sh
ESDIAG_E2E_RUN_ID=manual-001 ./tests/bin/esdiag-cli-e2e.sh
ESDIAG_E2E_JOB_NAME=test-job ./tests/bin/esdiag-cli-e2e.sh
ESDIAG_E2E_CLEAN_REMOTE=false ./tests/bin/esdiag-cli-e2e.sh
ESDIAG_E2E_PROCESS_KIBANA=true ./tests/bin/esdiag-cli-e2e.sh
```

`ESDIAG_E2E_PROCESS_KIBANA=true` is reserved for validating Kibana diagnostic
processing once that processor is implemented. By default the suite still
collects from Kibana but skips processing that collected Kibana diagnostic so
the release gate only covers supported workflows.

## Agent Skill Plugin Suite

`tests/plugin.sh` is a non-networked suite for the portable Agent Skill and host manifests under
`plugin/`. It needs no deployment, no credentials, and no container runtime, and
covers:

- packaging: the bundled skill, scripts, and references match
  `.agents/skills/esdiag/`, drift is detected, and the Claude, Codex, and
  marketplace manifests agree
- client configuration resolution: agent and space defaults, overrides, space
  scoping, the already-suffixed URL case, and omitted model routing
- API key handling: file references resolve and key values never appear in
  resolved output
- `esdiag` output parsing for `diagnostic.id` and `Kibana Link`
- streaming behavior against recorded event fixtures, including progress
  reporting, dashboard link resolution, conversation persistence, and the
  interrupted-stream path that must not re-run a paid analysis
- structural guards that the load-bearing rules in the skill instructions, such as
  intent classification and confirm-before-collecting, have not been dropped

Run it from the repository root:

```sh
./tests/plugin.sh
./tests/plugin.sh --only test_config_applies_default_agent
```

The suite requires `jq` and `shellcheck` is recommended for local linting.
Behavioral evaluation of the command prompts themselves is not covered here; the
suite asserts the rules are present, not that a model follows them.

## Fixture Archive Regeneration

`tests/bin/regenerate-fixture-archives.sh` rebuilds the checked-in
Elasticsearch, Kibana, and Logstash diagnostic archive fixtures under
`tests/archives/`.

Run it from the repository root:

```sh
./tests/bin/regenerate-fixture-archives.sh
```

Pass one or more Elastic Stack versions to regenerate a smaller set:

```sh
./tests/bin/regenerate-fixture-archives.sh 8.19.3 9.3.3
```

The script requires `cargo`, `curl`, and `docker` by default. Set
`CONTAINER_RUNTIME=podman` to use Podman instead. It starts temporary Elastic
Stack containers and uses a temporary directory for the Logstash pipeline
configuration.
