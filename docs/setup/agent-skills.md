---
type: Guide
title: Use ESDiag from a coding agent
description: Install the ESDiag Agent Skill, prepare saved hosts, and ask a coding agent to collect and share diagnostics.
tags: [setup, agent-skills, collection, upload]
---

# Use ESDiag from a coding agent

The ESDiag Agent Skill lets Claude Code, Codex, or OpenCode run `esdiag` for
you. You ask in plain language, such as "collect a diagnostic from my prod
cluster", and the agent runs the matching command with your saved hosts.

The skill calls the installed `esdiag` binary. It is not a separate runtime,
and it never asks for credentials. You set up hosts and credentials in your own
terminal; the agent only refers to them by name.

## What you need

- The [native binary](installation.md#native-binary).
- Claude Code, Codex, or OpenCode.
- A saved host for each cluster you want the agent to reach.

## Install the skill

```sh
esdiag agent skills
```

ESDiag installs its embedded skill for every coding agent it detects in your
home directory. To pick agents yourself, pass one or more targets:

```sh
esdiag agent skills --target claude --target codex
```

The targets are `claude`, `codex`, and `opencode`. Restart the agent so it
loads the skill.

`esdiag init` also offers to install the skill at the end of setup. Run
`esdiag agent skills` again after you upgrade ESDiag to install the matching
skill version. If you edited an installed copy, ESDiag leaves it alone unless
you add `--force`.

## Unlock the keystore

The agent cannot type your keystore password. Before secret-backed work, the
skill runs `esdiag keystore status` and asks you to unlock the keystore if it
is locked. Unlock it in your own terminal before you start:

```sh
esdiag keystore unlock
```

This creates the keystore if you don't have one yet. The unlock lasts 24
hours. Use `--ttl` to choose another duration, such as `--ttl 8h`, and
`esdiag keystore lock` to end it early.

## Save your clusters as hosts

The agent finds a cluster by its saved host name, so name each host the way
you will refer to it in a prompt. A host named `prod` lets you say "my prod
cluster".

Save a cluster with its credential:

```sh
esdiag keystore add prod --apikey
esdiag host add prod https://prod.example.com:9200
```

`host add` uses the keystore entry with the same name as the host. See
[Save a source](collect-and-share.md#save-a-source) for Kibana, Logstash, and
username-and-password sources, or run `esdiag init` to do the same steps
interactively.

Check the names the agent will see:

```sh
esdiag host list
```

## Collect and upload a diagnostic

Ask the agent:

```text
Collect a diagnostic from my prod cluster and upload it to <UPLOAD_ID>.
```

The agent runs a command like this one:

```sh
esdiag collect prod "$HOME/diagnostics" --upload '<UPLOAD_ID>'
```

ESDiag keeps the archive on disk and reports its path and the upload result.
Name an output directory in the prompt if you want the archive somewhere
specific. Ask for a `minimal`, `light`, or `support` diagnostic to change the
collection level from the default `standard`.

Upload IDs work like short-lived tokens, so you can give one to the agent. If
the upload fails because the ID has expired, ask for a new one and have the
agent upload the saved archive:

```text
Upload ~/diagnostics/prod-diagnostic.zip to <NEW_UPLOAD_ID>.
```

## Process and analyze

With a diagnostic cluster configured, the agent can also process archives and
ask Agent Builder about them:

```text
Process ~/diagnostics/prod-diagnostic.zip and tell me the highest-risk finding.
```

```text
Run my saved prod job, then summarize any red indices.
```

Set up the diagnostic cluster first with
[a local stack](local-stack.md), [a remote cluster](remote-cluster.md), or
[a Serverless project](serverless.md).

## Keep credentials out of the conversation

- Never paste API keys, passwords, or the keystore password into the agent.
- Run `esdiag init`, `esdiag keystore`, and `esdiag host add` in your own
  terminal.
- If the agent asks for a credential, decline and save it in the keystore
  instead.
