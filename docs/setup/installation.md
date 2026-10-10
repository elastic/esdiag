---
type: Reference
title: Install ESDiag
description: Install the native binary, container image, local-stack launcher, or coding-agent skill.
tags: [setup, installation, onboarding]
---

# Install ESDiag

Every [setup guide](index.md) links here for the details of its install step.
Install the native binary unless your guide tells you otherwise.

| Method | Gives you | Used by |
|---|---|---|
| [Native binary](#native-binary) | Every CLI command, the web UI, and the local-stack commands | All guides |
| [Container image](#container-image) | The web UI against a remote cluster or Serverless project | [Remote cluster](remote-cluster.md), [Serverless](serverless.md), [shared service](shared-service.md#service-administrators) |
| [Local-stack launcher](#local-stack-launcher) | A full containerized local stack without the binary | [Local stack](local-stack.md#containerized-full-mode) |
| [Coding-agent skill](#coding-agent-skill) | A coding agent that runs the installed binary | Any guide, as an add-on |

Shared-service users do not install anything. Open the URL from your
administrator. See [Use a shared service](shared-service.md).

## Native binary

Install a published release with Homebrew:

```sh
brew install elastic/tools/esdiag
```

Or install from source with Cargo:

```sh
cargo install esdiag
```

You can also download an archive and its checksum from the
[latest GitHub release](https://github.com/elastic/esdiag/releases/latest).
Verify the checksum, put `esdiag` on `PATH`, then check the installation:

```sh
esdiag --version
esdiag --help
```

For a user-scoped archive installation:

```sh
mkdir -p "$HOME/.local/bin"
tar -xzf esdiag-<version>-<target>.tar.gz
install -m 0755 esdiag "$HOME/.local/bin/esdiag"
export PATH="$HOME/.local/bin:$PATH"
```

Update the binary the same way you installed it. `esdiag local update` only
prints this guidance.

## Container image

The ESDiag image runs `esdiag` as its entrypoint. Use it to run the web UI
against a diagnostic cluster that already exists. It requires Podman or Docker.

```sh
docker pull docker.elastic.co/esdiag/esdiag:<version>
```

Replace `<version>` with the ESDiag release you want. The container reads its
diagnostic cluster from `ESDIAG_OUTPUT_*` and `ESDIAG_KIBANA_URL` environment
variables. Each guide shows the full `docker run` command for its target.

For a local stack, do not run the image yourself. Use
`esdiag local up --stack=full` or the [local-stack launcher](#local-stack-launcher),
which start the image with Elasticsearch and Kibana.

## Local-stack launcher

Use the standalone `esdiag-local` launcher when you want a containerized
local stack without installing the binary. It requires Podman or Docker with
Compose support.

On Windows, use WSL on WSL (Linux ESDiag and a container engine in the same
WSL distribution), or Hyper-V on PowerShell (native Windows ESDiag and Windows
Podman with a Hyper-V machine). See [Windows container paths](local-stack.md#windows-container-paths)
for setup. Native Windows configuration defaults to
`%USERPROFILE%\.esdiag`; setting `HOME` is unnecessary. The standalone Bash
launcher below is used from WSL on Windows.

```sh
mkdir -p "$HOME/.local/bin"
curl -fsSL \
  https://github.com/elastic/esdiag/releases/latest/download/esdiag-local \
  -o "$HOME/.local/bin/esdiag-local"
curl -fsSL \
  https://github.com/elastic/esdiag/releases/latest/download/esdiag-local.sha256 \
  -o "$HOME/.local/bin/esdiag-local.sha256"
chmod 755 "$HOME/.local/bin/esdiag-local"
```

Verify the download:

```sh
(
  cd "$HOME/.local/bin"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum --check esdiag-local.sha256
  else
    shasum -a 256 -c esdiag-local.sha256
  fi
)
```

The launcher can start a full stack alone. Install the matching `esdiag` binary
as well if you want CLI commands, core mode, or the coding-agent skill. See the
[local-stack launcher reference](../bin/esdiag-local.md) for every option.

## Coding-agent skill

The skill calls the installed `esdiag` binary. It is not a separate runtime.
Install the binary first, then install its embedded skill:

```sh
esdiag agent skills
```

Use `--target claude`, `--target codex`, or `--target opencode` to install the
skill for a specific agent. Restart the agent to reload the skill.

After a person has run `esdiag init`, the skill can run saved jobs, process
archives, and ask Agent Builder questions. It checks the keystore before
secret-backed work. If the keystore is locked, unlock it locally.

The skill never asks for credentials. Run `esdiag init` yourself in a terminal,
and never paste credentials into an agent conversation. See
[Use ESDiag from a coding agent](agent-skills.md) for host setup and sample
prompts.
