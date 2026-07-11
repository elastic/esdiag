---
type: Guide
title: Standalone Local ESDiag Stack
description: Run ESDiag, Elasticsearch, and Kibana from official container images.
tags: [bin, containers, deployment]
---

Standalone Local ESDiag Stack
=============================

Download `esdiag-local` from [ela.st/esdiag-local](https://ela.st/esdiag-local).
The single Bash 3.2+ artifact embeds its Compose configuration and runs from any
directory on Linux, macOS, and WSL. It prefers Podman, falls back to Docker, and
supports `--runtime podman|docker` for an explicit choice.

Quick start
-----------

```sh
chmod +x esdiag-local
./esdiag-local up
```

The stack uses pinned official images, binds Elasticsearch (`9200`), Kibana
(`5601`), and ESDiag (`2501`) to `127.0.0.1`, and always enables security.
The script attempts to copy the Kibana password immediately before opening the browser; disable that with
`--copy-password=false` or browser launch with `--open-browser=false`.

State and lifecycle
-------------------

Generated `.env`, `compose.yml`, and failure logs live in
`${ESDIAG_LOCAL_DIR:-~/.esdiag/local}`. Override this with `--state-dir`. The
directory is private and `.env` is mode `0600`. Edit the three documented port
values or `LOG_LEVEL` in `.env` before `up`; ports must be in range, unique, and available.
Initialized credentials and volumes are treated as one deployment state. If
either side is missing, startup fails closed and requires restoring the missing
state or running a confirmed reset.

Image defaults are version-pinned. Use `--image` for a complete ESDiag image
override, `--esdiag-registry` or `--elastic-registry` for registry overrides,
and `--esdiag-version` or `--elastic-version` for explicit stack versions.
`ESDIAG_IMAGE_TAG` takes precedence over stored and embedded ESDiag images.
Pull behavior is selected with `--pull always|missing|never`.

```sh
./esdiag-local status
./esdiag-local logs
./esdiag-local restart esdiag --log-level debug
./esdiag-local restart elasticsearch kibana
./esdiag-local setup
./esdiag-local auth
./esdiag-local down
```

`down` keeps configuration, credentials, and volumes. `reset` destroys all of
them and prompts for confirmation; automation must pass `reset --force`.
`restart` recreates only the named `elasticsearch`, `kibana`, or `esdiag`
services so persisted configuration changes take effect. `--log-level` stores
an ESDiag log filter such as `debug` in the deployment state. The `LOG_LEVEL`
environment variable provides the same setting; the CLI option takes precedence.

Credentials
-----------

The following commands emit exactly one raw secret on standard output. Errors
go to standard error. Ordinary status and log commands do not print secrets.

```sh
./esdiag-local secrets password
./esdiag-local secrets apikey
./esdiag-local secrets password | pbcopy  # macOS
```

Updates
-------

`update --check` checks official `github.com/elastic/esdiag` releases without
changing files. `update` downloads the stable `esdiag-local` and
`esdiag-local.sha256` assets, verifies the checksum and script, then atomically
replaces a writable regular installation. It refuses symlinks. Script updates do
not change an existing stack; use `up --upgrade` explicitly afterward.

The published checksum detects corruption or modification within the GitHub
release trust boundary. Because the checksum and artifact share that boundary,
it is not an independent cryptographic signature.
