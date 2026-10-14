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
./esdiag-local up
```

For installation and first use, see
[Run a local stack](../setup/local-stack.md).

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

On native Windows, run `esdiag local` from PowerShell with Windows Podman and
a Hyper-V machine. Under WSL, run the Linux binary with a container engine in
the same WSL distribution. See [Windows container paths](../setup/local-stack.md#windows-container-paths)
for the supported combinations. The Bash launcher
`esdiag-local` belongs to the WSL path on Windows.

The Windows local stack requires at least 8 GiB of Podman machine memory.
For Hyper-V, use fixed memory so the running guest retains the allocation;
see the Windows container guide above for configuration and verification.

Native Windows defaults to `%USERPROFILE%\.esdiag\local`; it does not require
`HOME`. Native run logs default to `%USERPROFILE%\.esdiag\last_run`.

The launcher writes generated `.env`, `compose.yml`, and logs to
`${ESDIAG_LOCAL_DIR:-~/.esdiag/local}`. Use `--state-dir` to change that path.
The directory is private and `.env` has mode `0600`.

The stack binds Elasticsearch to `127.0.0.1:9200`, Kibana to
`127.0.0.1:5601`, and the ESDiag web UI to `127.0.0.1:2501` by default.
Before opening the browser, `esdiag local up` and `esdiag local open` ask
whether to copy the generated Elastic password to the clipboard. Without an
interactive terminal they don't copy it. Pass `--copy-password=true` or
`--copy-password=false` to skip the question.

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
```

The update verifies the release checksum, then replaces a writable regular
script. It refuses symlinks. Updating the script does not change the stack's
image versions.

### Windows Subsystem for Linux

esdiag uses Windows browser and clipboard tools under WSL when Windows interoperability is enabled. Passwords are passed to clip.exe through standard input. If Windows tools cannot be launched, esdiag tries available Linux desktop tools and reports browser failures. Clipboard copying still requires the existing confirmation or explicit --copy-password=true option.

## Upgrades

`up` starts the image versions recorded in the state directory. When the
binary or script ships newer versions, `up` prints a warning and starts the
recorded versions anyway. `upgrade` moves the stack to the new versions:

```sh
esdiag local upgrade
esdiag-local upgrade
```

`upgrade` asks `[y/N]` before it changes anything, because Elasticsearch data
cannot be downgraded. Use `--force` in scripts and other non-interactive use.

- A running stack is restarted on the new images, and its ESDiag assets are
  reinstalled.
- A stopped stack records the new versions and stays stopped. The next `up`
  starts it on them.
- Core mode upgrades Elasticsearch and Kibana. Full mode also upgrades the
  ESDiag image.
- The stack keeps its mode. Change modes with `up --stack=<mode>`.
- If the image pull fails, the previous versions stay recorded.
- A stack that records a newer Elastic version than the tool ships is refused.

`esdiag-local upgrade` accepts `--elastic-version` and `--esdiag-version` to
choose target versions. On `up`, those options only apply to a new stack.
`up --upgrade` was replaced by `upgrade`.
