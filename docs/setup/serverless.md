---
type: Guide
title: Use a Serverless project
description: Process and analyze diagnostics in an Elasticsearch Serverless project.
tags: [setup, serverless, elasticsearch, exporter]
---

# Use a Serverless project

Use this guide when processed diagnostics belong in an Elasticsearch Serverless
project. The steps match [Use a remote cluster](remote-cluster.md), except for
asset setup. This page covers the whole journey and links there for shared
details.

## What you need

- The project's Elasticsearch and Kibana endpoints.
- An API key that can write `*-esdiag` indices.
- An API key that can install templates, ingest pipelines, and roles when the
  ESDiag assets are missing. A project administrator can run that step for you.
- A project feature tier that includes Agent Builder, if you need it.

## Install

Install the [native binary](installation.md#native-binary) for the CLI, saved
jobs, and the coding-agent skill. Use the
[container image](installation.md#container-image) if you only need the web UI.

## Connect the project

Run the guided setup:

```sh
esdiag init
```

Choose processing, then choose **remote**, and enter the project's
Elasticsearch and Kibana endpoints. Asset setup detects Serverless
automatically.

To configure the project manually, save it as a `send` host:

```sh
esdiag keystore unlock
esdiag keystore add diagnostics-project --apikey
esdiag host add diagnostics-project https://<project-elasticsearch-endpoint> \
  --app elasticsearch --roles send
esdiag host auth diagnostics-project
```

For the container, put the project endpoints and API key in `esdiag.env`, as
shown in [Use a remote cluster](remote-cluster.md#container).

## Install assets

```sh
esdiag setup diagnostics-project
```

For the container:

```sh
docker run --rm --env-file esdiag.env \
  docker.elastic.co/esdiag/esdiag:<version> setup
```

On Serverless, `setup` changes its behavior:

- It omits ILM settings, which Serverless does not support.
- Ordinary diagnostic data keeps a 30-day data stream retention policy.
  Diagnostic reports are retained indefinitely.
- It installs the bundled `esdiag-user` role. If the setup credential cannot
  manage roles, setup logs a warning and continues. Ask a project administrator
  to create the role.
- The ESDiag Kibana space uses the project's solution and feature visibility.
- Guided setup skips the stateful trial-license API. Agent Builder access
  depends on the project's feature tier.

Serverless security is always enabled. A `410 Gone` response from the security
usage API also reports security as enabled.

Run `esdiag setup` against the saved Kibana host to install the Kibana assets
separately. To choose the Kibana space, see
[Kibana space](remote-cluster.md#kibana-space).

ILM, shard, node, and snapshot dashboards still work for diagnostics collected
from stateful clusters. Those fields describe the source cluster, not the
Serverless project. See the
[Serverless asset audit](../reference/serverless-assets.md) for compatibility
details and repeatable verification.

If normal ingestion uses a less-privileged API key, replace the saved secret
after setup:

```sh
esdiag keystore update diagnostics-project --apikey
esdiag host auth diagnostics-project
```

## Process and analyze an archive

```sh
esdiag process /path/to/diagnostic.zip diagnostics-project
```

If Agent Builder is available in the project, ask a question as part of
processing:

```sh
esdiag process /path/to/diagnostic.zip diagnostics-project \
  --ask "What is the highest-risk finding, and what evidence supports it?"
```

The result includes a diagnostic ID and a Kibana conversation URL.

With the container, start the web UI as shown in
[Use a remote cluster](remote-cluster.md#process-and-analyze-an-archive), then
upload the archive on the **Process** page.

## Collect, process, and analyze

Save the cluster you want to diagnose as a `collect` host, as shown in
[Save a source](collect-and-share.md#save-a-source). Then run the job that
`esdiag init` saved:

```sh
esdiag job list
esdiag job run <NAME>
```

In the web UI, use the **Advanced** page as described in
[Use a remote cluster](remote-cluster.md#collect-process-and-analyze).

## Maintain

Rotate the API key:

```sh
esdiag keystore update diagnostics-project --apikey
```

To remove the project, first remove jobs and defaults that reference it:

```sh
esdiag host remove diagnostics-project
esdiag keystore remove diagnostics-project
```
