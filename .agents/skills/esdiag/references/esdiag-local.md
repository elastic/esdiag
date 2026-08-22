# `esdiag-local`

`esdiag-local` starts and manages a local container stack. It is not a native
ESDiag binary.

Start the container-only CLI path with a full stack:

```sh
esdiag-local up --stack=full
```

`exec` is full-mode-only. Core mode needs the native `esdiag` binary. It mounts
the current working directory for relative paths; pass `--mount /path` before
`--` for an archive or directory outside it.

## Collect

With `exec`, collect from a configured container-owned host:

```sh
esdiag-local exec -- collect <host> <directory>
```

Without `exec`, collection is unsupported. Use a native binary or ESDiag Lite.

## Share

With `exec`, upload diagnostics while collecting:

```sh
esdiag-local exec -- collect <host> <directory> --upload <upload-id>
```

Without `exec`, sharing is unsupported. ESDiag Lite can upload an existing ZIP
archive.

## Process

With `exec`, initialize the container-owned user state in an interactive
terminal:

```sh
esdiag-local exec -- init
```

Then process an archive visible to the command container:

```sh
esdiag-local exec -- process <input> [output]
```

Without `exec`, processing is unsupported. Use the web interface, a native
binary, or a hosted service.

## Analyze

With `exec`, ask Agent Builder:

```sh
esdiag-local exec -- agent ask "Analyze diagnostic <diagnostic-id>: <question>"
```

Without `exec`, CLI analysis is unsupported. Open the local web UI instead:

```sh
esdiag-local open
```

## Environment variables

`esdiag-local` reads a small launcher-specific set:

| Variable | Purpose |
|---|---|
| `ESDIAG_LOCAL_DIR` | Managed state directory. Defaults to `~/.esdiag/local`. |
| `ESDIAG_CONTAINER_RUNTIME` | Selects `podman` or `docker`. |
| `ESDIAG_IMAGE_TAG` | Overrides the ESDiag image for full-mode startup. |
| `LOG_LEVEL` | Sets ESDiag log verbosity during startup. |

The launcher writes generated credentials, image selections, ports, and stack
mode into its private `.env` file. Treat that file as managed state. Use
launcher options such as `--state-dir`, `--runtime`, and `--image` instead of
editing it.
