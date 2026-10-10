# Set up ESDiag

Each guide below covers one journey from installation to a result. Pick the
guide that matches where your processed diagnostics will live.

## Choose a guide

**Are you only collecting a diagnostic to share with someone else?**
Follow [Collect and share diagnostics](collect-and-share.md). You do not need a
diagnostic cluster.

**Do you need to process and analyze diagnostics?** Choose where the processed
documents go:

| Destination | Use it when | Guide |
|---|---|---|
| Local stack | You want Elasticsearch, Kibana, and ESDiag on your own machine. | [Run a local stack](local-stack.md) |
| Remote cluster | Your organization runs a self-managed or Elastic Cloud Hosted deployment for diagnostics. | [Use a remote cluster](remote-cluster.md) |
| Serverless project | Your diagnostics belong in an Elasticsearch Serverless project. | [Use a Serverless project](serverless.md) |
| Shared service | An administrator already runs ESDiag for your team. | [Use a shared service](shared-service.md) |

Each processing guide follows the same order:

1. Install ESDiag.
2. Connect the diagnostic cluster.
3. Process and analyze an archive you already have.
4. Collect directly from a cluster, then process and analyze in one step.
5. Operate and maintain the setup.

Stop after step 3 if you only analyze archives that other people send you.

## Native or containerized

| Install | Local stack | Remote cluster | Serverless project |
|---|---|---|---|
| Native binary | Core mode | CLI and web UI | CLI and web UI |
| Container | Full mode | Web UI | Web UI |

The native binary is the recommended starting point. It gives you every CLI
command, the web UI, and the coding-agent skill. The container image runs only
the web UI. A local stack always uses containers for Elasticsearch and Kibana.
The install choice only controls whether ESDiag itself runs natively or in a
container. See [Install ESDiag](installation.md) for every install method.

## Add-ons

- [Use ESDiag from a coding agent](agent-skills.md) to let Claude Code, Codex,
  or OpenCode collect, upload, and analyze diagnostics with your saved hosts.
- [Connect Agent Builder to a local LLM](local-llm.md) when you want to use an
  OpenAI-compatible model on your network.

## Terms used in these guides

- A `collect` host is the system ESDiag reads.
- A `send` host is the Elasticsearch destination for processed documents.
- A `view` host is the linked Kibana endpoint.
- `esdiag init` saves an interactive setup. `esdiag setup` installs or updates
  assets in a diagnostic cluster.

Collection does not process or upload an archive. Processing does not upload
the raw archive. Run the command for each destination you need. See
[Hosts and keystore](../hosts-keystore.md) for how saved hosts and credentials
work.
