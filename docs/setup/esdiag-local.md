---
type: Guide
title: Run a local diagnostic cluster
description: Run Elasticsearch, Kibana, and the ESDiag web UI on one machine.
tags: [setup, containers, local, agent-builder]
---

# Run a local diagnostic cluster

Run a local stack when you need local dashboards, browser submissions, or Agent
Builder. It needs Podman or Docker with Compose support, 4 GB of disk space,
and preferably 8 GB of memory.

Install the binary first. See [Install ESDiag](installation.md#binary).

## Start the stack

The binary owns the normal path:

```sh
esdiag local up
esdiag local auth
```

On a new state directory, `--stack=auto` starts core mode. Core mode runs
Elasticsearch and Kibana in containers and starts native `esdiag serve --mode
user`.

Choose a mode only when you need to:

```sh
esdiag local up --stack=core
esdiag local up --stack=full
```

Full mode runs the ESDiag web service in a container. A mode switch does not
move hosts, jobs, settings, or secrets between native state and the full-mode
container volume.

The standalone script remains available:

```sh
esdiag-local up --stack=full
esdiag-local auth
```

It can use core mode only when it finds an exactly matching native binary.

The stack binds these default endpoints to loopback:

- ESDiag: `http://127.0.0.1:2501`
- Elasticsearch: `http://127.0.0.1:9200`
- Kibana: `http://127.0.0.1:5601`

## Configure ESDiag

Run the initializer at an interactive terminal:

```sh
esdiag init
```

Choose local processing. If the stack already exists, the initializer reads its
generated endpoints and asks before installing assets. If it does not, the
initializer can start a core stack; that approval includes the new stack's
required assets. When it starts one, it opens the local web UI only after every
onboarding question has been completed.

The initializer creates native user configuration. The local stack keeps its
generated credentials separately under `~/.esdiag/local`.

Get a generated secret only when ESDiag prompts for it:

```sh
esdiag local secrets password
esdiag local secrets apikey
```

These commands print raw secrets. Do not capture their output in history,
documents, tickets, or chat. For a standalone stack, substitute
`esdiag-local`.

## Set up Agent Builder

You need an Enterprise license or trial and a configured inference model before
using `esdiag process --ask` or `esdiag agent ask`.

For Elastic Inference Service:

1. Sign in to `http://localhost:5601/s/esdiag`.
2. Open **Cloud Connect** from Kibana global search.
3. Sign in to the intended Elastic Cloud organization and connect Elastic
   Inference Service.
4. Open **AI Agent** and confirm that **Elastic AI Agent** is available in the
   `esdiag` space.

See Elastic's
[self-managed EIS setup](https://www.elastic.co/docs/explore-analyze/elastic-inference/connect-self-managed-cluster-to-eis)
for account and billing details. For a local OpenAI-compatible model, see the
[LLM configuration guide](../llm-setup-guide.md).

## Process an archive

```sh
esdiag process /path/to/elasticsearch-api-diagnostics.zip \
  --ask "What is the highest-risk finding, and what evidence supports it?"
```

The result includes the diagnostic ID and a Kibana conversation URL.

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

For a standalone stack, use the matching `esdiag-local` commands. The script
can check and install its own update:

```sh
esdiag-local update --check
esdiag-local update
esdiag-local up --upgrade
```

Update the binary through Homebrew, Cargo, or its release archive.
`esdiag local update` only prints that guidance.

For ports, registries, state paths, and every lifecycle option, see the
[local-stack launcher reference](../bin/esdiag-local.md).
