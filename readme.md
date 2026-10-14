# Elastic Stack Diagnostics

Elastic Stack Diagnostics (`esdiag`) collects diagnostic bundles from Elastic
Stack products, shares raw archives, processes diagnostic data into
Elasticsearch, and provides Kibana and Agent Builder analysis handoffs.

## Start here

The [setup overview](docs/setup/index.md) helps you pick a guide. Each guide
covers one journey, from installation to a result:

- [Collect and share diagnostics](docs/setup/collect-and-share.md) — no
  diagnostic cluster needed.
- [Run a local stack](docs/setup/local-stack.md) — process and analyze on your
  own machine.
- [Use a remote cluster](docs/setup/remote-cluster.md) — process and analyze in
  an existing deployment.
- [Use a Serverless project](docs/setup/serverless.md) — process and analyze in
  Elasticsearch Serverless.
- [Use a shared service](docs/setup/shared-service.md) — upload to an
  administrator-run ESDiag service.
- [Use ESDiag from a coding agent](docs/setup/agent-skills.md) — ask Claude
  Code, Codex, or OpenCode to collect, upload, and analyze diagnostics.

[Install ESDiag](docs/setup/installation.md) covers the native binary, container
image, local-stack launcher, and coding-agent skill.

## Local diagnostic cluster

With an installed binary, start the version-matched local stack in **core**
mode. Core mode runs Elasticsearch and Kibana containers alongside the native
ESDiag web UI:

```sh
esdiag local up
```

Script-first users can download the standalone `esdiag-local` release artifact
and start a **full** stack, which also runs ESDiag in a container:

```sh
./esdiag-local up --stack=full
```

Both paths use secure loopback-only defaults and shared stack state. Core and
full modes intentionally retain separate ESDiag user configuration; switching
modes does not migrate hosts, jobs, settings, or secrets.

See [Run a local stack](docs/setup/local-stack.md) for
prerequisites, credential handling, Agent Builder setup, and lifecycle
commands.

## Documentation

- [ESDiag Documentation](docs/documentation.md)
- [Command-Line Interface Reference](docs/command-line.md)
- [Local-Stack Launcher Reference](docs/bin/esdiag-local.md)
- [Desktop packaging guidance](docs/build/desktop-packaging.md)

## Development

Repository structure and contributor guidance are documented in
[Repository Organization](docs/repository/organization.md). Contributors can
build a source-image local stack with `./bin/esdiag-control up`; this is a
development workflow, not the user onboarding path.
