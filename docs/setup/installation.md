---
type: Guide
title: Install ESDiag
description: Install the binary, standalone launcher, or coding-agent skill.
tags: [setup, installation, onboarding]
---

# Install ESDiag

Install the binary if you will use the CLI, start a local stack, or use the
coding-agent skill.

## Binary

Install a published release with one of these commands:

```sh
brew install elastic/tools/esdiag
```

```sh
cargo install --locked esdiag
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

Replace the placeholders with the release asset you downloaded. Add the `PATH`
export to your shell startup file if needed.

## Standalone launcher

Use the standalone `esdiag-local` launcher when you want a script-first,
full-container local stack. It needs Podman or Docker with Compose support.

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
as well if you want CLI commands, core mode, or the coding-agent skill.

## Coding-agent skill

The skill calls `esdiag`. Install the binary first, then install its embedded
skill:

```sh
esdiag agent skills
```

Use `--target claude`, `--target codex`, or `--target opencode` when automatic
detection picks the wrong agent. Restart the agent after installation.

The skill does not accept credentials. Run `esdiag init` yourself at an
interactive terminal.

## Shared service

If your organization provides an ESDiag web service, open its URL. Do not
install the binary, launcher, or skill unless you also need a local workflow.
See [Use a shared service](shared-service.md).

## Next

[Configure ESDiag](configuration.md) for a local or remote workflow. Shared
service users can go straight to [Use a shared service](shared-service.md).
