# Test Utilities

This directory contains integration tests and opt-in test utilities for workflows
that need local services or externally managed Elastic Stack instances.

## esdiag-control Integration Script

`tests/bin/esdiag-control.sh` is an opt-in shell integration suite for the
user-facing `bin/esdiag-control` helper. It exercises the control script itself:

- runs `shellcheck` against `bin/esdiag-control`
- verifies help output
- builds the local `esdiag` container image
- starts and authenticates the local stack with security disabled
- runs setup against the started stack
- tears the stack down
- repeats startup, auth, setup, and teardown with security enabled

Run it from the repository root:

```sh
./tests/bin/esdiag-control.sh
```

To run one test function:

```sh
./tests/bin/esdiag-control.sh --only command_help_prints_usage
```

The script requires `shellcheck` and either `podman` or `docker`. It writes its
combined command log to `target/test-esdiag-control.log` and uses a temporary
`.env.test` file copied from `.env` or `example.env`.

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
