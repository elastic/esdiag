---
type: Guide
title: Local-stack launcher reference
description: Lifecycle and state reference for esdiag local and esdiag-local.
tags: [bin, containers, deployment]
---

# Local-stack launcher reference

Use the installed binary for a local stack:

```sh
esdiag local up
```

`esdiag local` owns its stack lifecycle directly in Rust. Update the binary
through Homebrew, Cargo, or its release archive; `esdiag local update` cannot
replace binary-owned code.

Use the standalone `esdiag-local` script when a script is your entry point. It
needs Bash 3.2 or later and Podman or Docker with Compose support.

```sh
./esdiag-local up --stack=full
```

For installation and first use, see
[Run a local diagnostic cluster](../setup/esdiag-local.md).

## Stack modes

`up` accepts `--stack=auto|core|full`.

| Mode | Services |
|---|---|
| `auto` | `esdiag local` uses core for a new state directory. `esdiag-local` uses core only with a matching native binary and otherwise uses full. |
| `core` | Runs Elasticsearch and Kibana containers, plus native `esdiag serve --mode user`. |
| `full` | Runs Elasticsearch, Kibana, and ESDiag containers. |

The state directory records the selected mode. Changing modes does not move
hosts, jobs, settings, or secrets between native user state and the full-mode
container volume. Existing state without a mode record is full mode.

## State

The launcher writes generated `.env`, `compose.yml`, and logs to
`${ESDIAG_LOCAL_DIR:-~/.esdiag/local}`. Use `--state-dir` to change that path.
The directory is private and `.env` has mode `0600`.

The stack binds Elasticsearch to `127.0.0.1:9200`, Kibana to
`127.0.0.1:5601`, and the ESDiag web UI to `127.0.0.1:2501` by default.
`esdiag local up` and `esdiag local open` copy the generated Elastic password
to the clipboard when a platform clipboard helper is available; pass
`--copy-password=false` to opt out.

The stack stores generated Elasticsearch credentials and API keys in `.env`.
Do not copy them into `hosts.yml`, `settings.yml`, or `secrets.yml`.

Core mode uses the native user's ESDiag state. Full mode uses the
`esdiag-data` container volume.

## Lifecycle commands

```sh
esdiag local status
esdiag local auth
esdiag local logs
esdiag local setup
esdiag local restart esdiag
esdiag local restart elasticsearch kibana
esdiag local down
```

`down` keeps state and volumes. `reset --force` removes containers,
credentials, and volumes:

```sh
esdiag local reset --force
```

Use `esdiag-local` in place of `esdiag local` for a standalone stack.

## Credentials and updates

These commands print one raw secret:

```sh
esdiag local secrets password
esdiag local secrets apikey
```

Do not capture the output in history, tickets, documents, or chat.

The standalone script can update itself:

```sh
esdiag-local update --check
esdiag-local update
esdiag-local up --upgrade
```

The update verifies the release checksum, then replaces a writable regular
script. It refuses symlinks. Updating the script does not upgrade a running
stack.
