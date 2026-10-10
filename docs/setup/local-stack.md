---
type: Guide
title: Run a local stack
description: Process and analyze diagnostics with Elasticsearch, Kibana, and ESDiag on one machine.
tags: [setup, containers, local, agent-builder]
---

# Run a local stack

Use this guide when you want dashboards, browser uploads, and Agent Builder on
your own machine. ESDiag starts Elasticsearch and Kibana in containers and
keeps every processed diagnostic on the machine.

You need Podman or Docker with Compose support, 4 GB of disk space, and
preferably 8 GB of memory.

## Install

Choose how ESDiag itself runs. Elasticsearch and Kibana run in containers in
both modes.

| Mode | ESDiag runs | Install | Choose it when |
|---|---|---|---|
| Core | Natively on your machine | [Native binary](installation.md#native-binary) | You want the CLI, the web UI, and the coding-agent skill. This is the default. |
| Full | In a container | [Native binary](installation.md#native-binary) or the [local-stack launcher](installation.md#local-stack-launcher) | You want everything containerized, or you cannot install the binary. |

Core and full modes keep separate ESDiag state. Switching modes does not move
hosts, jobs, settings, or secrets between the native user directory and the
full-mode container volume.

### Native (core mode)

```sh
esdiag local up
esdiag local auth
```

On a new state directory, `esdiag local up` starts core mode. Core mode runs
`esdiag serve --mode user` natively next to the Elasticsearch and Kibana
containers.

### Containerized (full mode)

```sh
esdiag-local up
esdiag-local auth
```

The launcher starts full mode. If the matching `esdiag` binary is also on your
`PATH`, the launcher uses core mode instead. For every other `esdiag local`
command in this guide, substitute `esdiag-local` when you use the launcher.

To run full mode from the binary, use `esdiag local up --stack=full`.

### Windows container paths

Use one of these supported execution paths:

| Path | ESDiag command environment | Container engine |
|---|---|---|
| WSL on WSL | Linux `esdiag` in the WSL shell | Podman or Docker installed in that same WSL distribution |
| Hyper-V on PowerShell | Native Windows `esdiag.exe` in PowerShell | Windows `podman.exe` connected to a Hyper-V Podman machine |

Keep the command and container runtime in the corresponding path. Mixing a
Windows ESDiag process with a WSL-backed runtime, or a WSL ESDiag process with
the Windows runtime, is not a supported local-stack path. Windows Podman's
WSL-backed machine is distinct from Podman installed in your WSL distribution.

For the PowerShell path, create and start a Hyper-V machine with Windows
Podman, then select its connection before running `esdiag local up`. See the
[Podman Windows guide](https://github.com/podman-container-tools/podman/blob/main/docs/tutorials/podman-for-windows.md)
for Hyper-V prerequisites and machine creation. `podman info` must succeed,
and published ports must be reachable from PowerShell.

Allocate at least **8 GiB of memory to the Podman machine** for the Windows
local stack. For Hyper-V, `podman machine init --memory 8192` specifies 8192
MiB (8 GiB). Ensure the running guest actually receives that memory: Hyper-V
dynamic memory can reduce the guest below the configured startup allocation
and cause Elasticsearch or Kibana to run out of memory during startup.
Use fixed memory for this stack. With the Podman machine stopped, run from
an administrator PowerShell window, substituting your machine name:

```powershell
Set-VMMemory -VMName podmachine-hyperv -DynamicMemoryEnabled $false -StartupBytes 8GB
```

Restart it with `podman machine start podmachine-hyperv`. Verify available
guest memory with `podman machine ssh podmachine-hyperv free -m`. For WSL on
WSL, ensure the WSL environment hosting the engine has at least 8 GiB available.

Native Windows user configuration lives in `%USERPROFILE%\.esdiag`, and local
stack state defaults to `%USERPROFILE%\.esdiag\local`. No `HOME` variable is
required. WSL uses its Linux user's `~/.esdiag` directory independently.
`ESDIAG_HOME` overrides the run-log directory's base: an absolute path is used
directly, and a relative path is resolved beneath the native user directory.
Use `ESDIAG_LOCAL_DIR` or `--state-dir` to override local-stack state.

Core mode tracks its native Windows web service by executable path and process
creation time. `esdiag local restart esdiag` replaces that service, and
`esdiag local down` stops it before tearing down the containers. Windows
shutdown terminates the verified process and waits for it to exit.

### Endpoints

The stack binds these default endpoints to loopback:

- ESDiag: `http://127.0.0.1:2501`
- Elasticsearch: `http://127.0.0.1:9200`
- Kibana: `http://127.0.0.1:5601`

When `esdiag local up` opens a browser, it starts at the ESDiag onboarding page,
`http://127.0.0.1:2501/welcome`. Use `esdiag local open` to open the web UI
root instead.

## Connect ESDiag to the stack

Finish onboarding in the terminal or the browser. In core mode, both save the
same state. In full mode, terminal `esdiag init` writes the host's `~/.esdiag`,
while browser onboarding writes the container's `esdiag-data` volume, so finish
full-mode onboarding in the browser.

In the terminal, run:

```sh
esdiag init
```

Choose local processing. If the stack already exists, the initializer reads its
generated endpoints and asks before installing assets. If it does not, the
initializer can start a core stack. That approval includes the new stack's
required assets. If you decline, it returns to remote setup without creating
local state. After a terminal setup that started a stack, `init` asks whether
to open the web UI. The default is no.

In the browser, complete the `/welcome` page. Submitting each stage updates
the current page directly, including when a diagnostic user was already
configured by terminal setup. At the **Diagnostic Source** stage, choose
**Add later** if you only process archives. You can return to `/welcome` to
add a source.

A full-mode stack identifies itself to the web UI, so onboarding uses the
stack's own Elasticsearch and Kibana as the diagnostic cluster. It never tries
to start containers from inside the ESDiag container. Without the binary, the
browser is the only way to finish onboarding.

The local stack keeps its generated credentials under `~/.esdiag/local`. Print
one only when ESDiag prompts for it:

```sh
esdiag local secrets password
esdiag local secrets apikey
```

These commands print raw secrets. Do not capture their output in history,
documents, tickets, or chat.

## Set up Agent Builder

You need an Enterprise license or trial and a configured inference model before
you can ask Agent Builder questions.

For Elastic Inference Service:

1. Sign in to `http://localhost:5601/s/esdiag`.
2. Open **Cloud Connect** from Kibana global search.
3. Sign in to the intended Elastic Cloud organization and connect Elastic
   Inference Service.
4. Open **AI Agent** and confirm that **Elastic AI Agent** is available in the
   `esdiag` space.

See Elastic's
[self-managed EIS setup](https://www.elastic.co/docs/explore-analyze/elastic-inference/connect-self-managed-cluster-to-eis)
for account and billing details. For a local OpenAI-compatible model, see
[Connect Agent Builder to a local LLM](local-llm.md).

## Process and analyze an archive

In the web UI, open `http://127.0.0.1:2501`, upload the archive on the
**Process** page, and follow the returned Kibana link.

From the CLI:

```sh
esdiag process /path/to/diagnostic.zip
```

If Agent Builder is ready, ask a question as part of processing:

```sh
esdiag process /path/to/diagnostic.zip \
  --ask "What is the highest-risk finding, and what evidence supports it?"
```

The result includes the diagnostic ID and a Kibana conversation URL.

## Collect, process, and analyze

Save the cluster you want to diagnose as a `collect` host. Use the web UI's
**Settings** page, or follow [Save a source](collect-and-share.md#save-a-source).
In full mode, the source must be reachable from inside the ESDiag container.

In the web UI, open **Advanced**. In **Collect**, choose **New**, then **Known
Host**. Leave **Process** enabled, choose the default diagnostic cluster in
**Send**, then select **Collect**. Add an **Elastic Upload Service ID** under
**Raw bundle delivery** to share the raw archive too.

From the CLI, run the job that `esdiag init` saved:

```sh
esdiag job list
esdiag job run <NAME>
```

To keep the raw archive, collect first and pass the reported path to `process`.
ESDiag only uploads when you run `upload` or use `collect --upload`.

## Operate the stack

```sh
esdiag local status
esdiag local logs
esdiag local setup
esdiag local restart esdiag --log-level debug
esdiag local down
```

`down` keeps the state and volumes. `reset --force` removes containers,
credentials, and volumes:

```sh
esdiag local reset --force
```

## Upgrade the stack

A stack keeps the Elasticsearch, Kibana, and ESDiag image versions it was
created with. When a newer binary or launcher ships newer versions, `up` warns
and keeps starting the recorded versions. Upgrade when you are ready:

```sh
esdiag local upgrade
```

`upgrade` asks for confirmation because Elasticsearch data cannot be downgraded
afterward. Pass `--force` for non-interactive use. It pulls the new images,
restarts a running stack on them, and reinstalls the ESDiag assets. A stopped
stack stays stopped and starts on the new versions at the next `up`. Core mode
upgrades only Elasticsearch and Kibana, since the native binary is the ESDiag
web service.

Update the binary through Homebrew, Cargo, or its release archive, then run
`esdiag local upgrade`. `esdiag local update` only prints that guidance.

The launcher updates itself, then upgrades its stack the same way:

```sh
esdiag-local update --check
esdiag-local update
esdiag-local upgrade
```

For ports, registries, state paths, and every lifecycle option, see the
[local-stack launcher reference](../bin/esdiag-local.md).
