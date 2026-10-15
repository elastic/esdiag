# First-run setup

Ask one question first: will the user only collect and share diagnostics, or
will they also process them?

Then check for a native binary:

```sh
command -v esdiag
```

An installed binary skips installation. Move to [choose an interface](#choose-an-interface).

## Collect and share only

Ask whether the user can install a native binary.

If they can, use [install a native binary](#install-a-native-binary). A native
binary is the route to saved jobs, processing, and the Agent Skill later.

If they cannot, use ESDiag Lite. It collects Elasticsearch diagnostics and can
send a ZIP archive to Elastic Upload Service. It does not process diagnostics,
run a web UI, or call
Agent Builder.

- Linux and macOS use `esdiag-lite.sh`.
- Windows uses `esdiag-lite.ps1`.

Provide the script through the user's approved source. Do not invent a raw
download URL or ask for Elasticsearch credentials in chat. The user supplies
connection details in their terminal.

Collection ends this branch. The user can decide how to share the archive after
collection.

## Process diagnostics

Ask which runtime the user can use: a native binary, a local container stack,
or an administrator-provided hosted service.

### Native binary

Use [install a native binary](#install-a-native-binary), then [choose an
interface](#choose-an-interface).

### Local container stack

Check for a usable container runtime and Compose support:

```sh
if command -v podman >/dev/null 2>&1; then
  podman compose version
elif command -v docker >/dev/null 2>&1; then
  docker compose version
else
  exit 1
fi
```

If neither runtime works, return to the native-binary choice. Do not offer a
local container workflow that cannot run.

Download, verify, and start the full stack:

```sh
mkdir -p "$HOME/.local/bin"
curl -fsSL https://github.com/elastic/esdiag/releases/latest/download/esdiag-local \
  -o "$HOME/.local/bin/esdiag-local"
curl -fsSL https://github.com/elastic/esdiag/releases/latest/download/esdiag-local.sha256 \
  -o "$HOME/.local/bin/esdiag-local.sha256"
chmod 755 "$HOME/.local/bin/esdiag-local"
(
  cd "$HOME/.local/bin"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum --check esdiag-local.sha256
  else
    shasum -a 256 -c esdiag-local.sha256
  fi
)
"$HOME/.local/bin/esdiag-local" up --stack=full
```

For a web-first workflow, direct the user to:

```sh
"$HOME/.local/bin/esdiag-local" open
```

For CLI-first container work, initialize and run commands through the
full-mode service:

```sh
"$HOME/.local/bin/esdiag-local" exec -- init
"$HOME/.local/bin/esdiag-local" exec -- process ./diagnostic.zip
```

`exec` is full-mode-only. It preserves the container-owned ESDiag state and
mounts the current directory for relative paths. Add `--mount /path` before
`--` for an archive outside the working directory.

### Hosted service

Use the administrator-provided ESDiag and Kibana URLs. Do not install a local
stack unless the user also wants one. A CLI or Agent Skill workflow still needs
a native binary or an administrator-provided runner.

## Install a native binary

Check which installation method the user can use:

```sh
command -v brew
command -v cargo
```

Ask which method they prefer when both are present. With their approval:

```sh
brew install elastic/tools/esdiag
```

```sh
cargo install --locked esdiag
```

GitHub release archives are another native-install path on platforms with a
published archive. Direct the user to the official release page and their
platform's matching asset. Do not guess an asset name.

Confirm the installation:

```sh
esdiag --version
```

## Choose an interface

Ask whether the user will mainly use the CLI, including the Agent Skill, or the
web interface.

For CLI-first work, direct the user to run this in their own terminal:

```sh
esdiag init
```

The wizard keeps credentials local. It can configure collection-only work,
remote processing, or a local core stack.

For web-first local processing, direct the user to start and open the core
stack:

```sh
esdiag local up --stack=core
esdiag local open
```

For a hosted workflow, use the URL provided by the administrator.

## Guardrails

Keep passwords, API keys, and keystore values in the user's terminal. Do not
request them in chat. Do not write ESDiag state files by hand.
