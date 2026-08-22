---
type: Guide
title: esdiag-control
description: Build ESDiag from this checkout and run it in a local stack.
tags: [bin, containers, deployment]
---

# esdiag-control

`bin/esdiag-control` is for repository contributors. It builds the image from
the current checkout, then delegates stack lifecycle commands to
`bin/esdiag-local`.

For published images without a source checkout, use
[esdiag local](../setup/esdiag-local.md) or the standalone launcher.

Run the script from the repository root. It needs Podman or Docker. It refuses
to run as root.

## Start a contributor stack

```sh
./bin/esdiag-control up
```

The command builds `esdiag:<version>` for the current checkout, starts the
local stack without pulling an ESDiag image, and stores its state in
`target/esdiag-local`.

Use `--state-dir` or `ESDIAG_LOCAL_DIR` to change the state directory:

```sh
./bin/esdiag-control up --state-dir /path/to/state
```

## Commands

| Command | What it does |
|---|---|
| `build` | Builds an image for the current platform. |
| `buildx` | Builds images for `linux/amd64` and `linux/arm64`. |
| `up` | Builds the image and starts the delegated local stack. |
| `down` | Stops the delegated stack. |
| `setup` | Runs delegated asset setup. |
| `auth` | Tests delegated Elasticsearch and Kibana authentication. |

`build` and `buildx` reuse an existing image with the current Cargo version.
Pass `--push` to publish it. Set `--registry <URL>` or `ESDIAG_REGISTRY` to
choose the registry.

```sh
./bin/esdiag-control build
./bin/esdiag-control buildx --push --registry registry.example.com
./bin/esdiag-control auth
./bin/esdiag-control down
```

## Shared options

```text
--runtime podman|docker
--registry URL
--push
--open-browser=true|false
--state-dir DIR
```

Podman is the first runtime the script detects. Docker is the fallback. Use
`--runtime` to choose one explicitly.

Run `./bin/esdiag-control help` for the script's current options.
