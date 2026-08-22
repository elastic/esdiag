# Elastic Stack Diagnostics

Elastic Stack Diagnostics (`esdiag`) collects diagnostic bundles from Elastic
Stack products, shares raw archives, processes diagnostic data into
Elasticsearch, and provides Kibana and Agent Builder analysis handoffs.

## Start here

Choose the stages you need; installation, diagnostic-cluster setup, and daily
usage are separate decisions.

1. [Install ESDiag](docs/setup/installation.md) — binary-first,
   container-first, skill-first, or no installation for a shared service.
2. [Configure ESDiag](docs/setup/configuration.md) — no diagnostic cluster,
   a local diagnostic cluster, a remote diagnostic cluster, or a shared hosted
   service.
3. [Use ESDiag](docs/setup/usage.md) — collect and share, process and analyze,
   or run every stage through the CLI, web UI, or coding-agent skill.

The [setup overview](docs/setup/index.md) maps the three main user workflows:

- Collect and share diagnostics
- Process and analyze diagnostics
- Collect, share, process, and analyze diagnostics

## Local diagnostic cluster

With an installed binary, start the version-matched local stack:

```sh
esdiag local up
```

New automatic deployments select native **core** mode when the binary is
compatible. Core mode runs Elasticsearch and Kibana containers while managing
the native ESDiag web UI. Use `--stack=full` for the fully containerized ESDiag
web service:

```sh
esdiag local up --stack=full
```

Script-first users can download the standalone `esdiag-local` release artifact
and run:

```sh
./esdiag-local up --stack=full
```

Both paths use secure loopback-only defaults and shared stack state. Core and
full modes intentionally retain separate ESDiag user configuration; switching
modes does not migrate hosts, jobs, settings, or secrets.

See [Run a Local Diagnostic Cluster](docs/setup/esdiag-local.md) for
prerequisites, credential handling, Agent Builder setup, and lifecycle
commands.

## Documentation

- [ESDiag Documentation](docs/documentation.md)
- [Command-Line Interface Reference](docs/command-line.md)
- [Local-Stack Launcher Reference](docs/bin/esdiag-local.md)
- [Use an Existing Cluster](docs/setup/existing-cluster.md)
- [Use a Shared ESDiag Service](docs/setup/shared-service.md)
- [Desktop packaging guidance](docs/build/desktop-packaging.md)

## Development

Repository structure and contributor guidance are documented in
[Repository Organization](docs/repository/organization.md). Contributors can
build a source-image local stack with `./bin/esdiag-control up`; this is a
development workflow, not the user onboarding path.
