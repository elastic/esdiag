---
type: Guide
title: Use an existing cluster
description: Send processed diagnostics to an existing Elasticsearch deployment.
tags: [setup, elasticsearch, exporter, hosts]
---

# Use an existing cluster

Use this path when processed documents belong in an existing Elasticsearch
deployment. Install the binary first. See [Install ESDiag](installation.md#binary).

The guided route is:

```sh
esdiag init
```

Choose processing, then choose **remote**. `init` saves the Elasticsearch
destination, optional Kibana viewer, encrypted credentials, and default output.
It can also install assets and configure collection.

Use the steps below when the cluster administrator and the ESDiag user have
separate responsibilities.

## What you need

- Network access to the Elasticsearch output.
- A credential that can write `*-esdiag` indices.
- An administrator credential to install ESDiag assets when they are missing.
- The matching Kibana URL and a configured inference model if you need
  dashboards or Agent Builder.

A `collect` host is the source of a raw archive. A `send` host is the
Elasticsearch output. A `view` host is the linked Kibana endpoint. One
Elasticsearch endpoint can have both `collect` and `send` roles, but separate
credentials are usually clearer when different teams own the systems.

## Save the output

Create or unlock the keystore:

```sh
esdiag keystore unlock
```

Add the output credential through a masked prompt:

```sh
esdiag keystore add diagnostics-output --apikey
```

Save the Elasticsearch host and test it:

```sh
esdiag host add diagnostics-output https://diagnostics.example.com:9200 \
  --app elasticsearch --roles send --secret diagnostics-output
esdiag host auth diagnostics-output
```

Do not use a raw HTTP URL as a `process` output. Save it as a host so ESDiag can
validate the role and resolve its credentials.

## Install assets

Kibana assets install into the `esdiag` space unless `ESDIAG_KIBANA_SPACE`
selects another destination. To install into Kibana's default space:

```sh
ESDIAG_KIBANA_SPACE=_default esdiag setup esdiag-less
```

Replace `esdiag-less` with your configured deployment or Kibana host name.
`_default` skips space creation and omits `/s/{space}` from asset requests and
links. An empty value or Kibana's `default` ID also selects the default space. A named value such as
`support` installs into that space. An explicit selection overrides a space
prefix in the configured Kibana URL. Keep the same environment setting when
running `process`, `serve`, or `agent ask` so their links use that destination.

Install ESDiag assets into only one Kibana space. If they already exist in
another space, setup stops before importing and names that space. Set
`ESDIAG_KIBANA_SPACE` to that space to update it, or remove the assets from it
before installing elsewhere.

The output needs ESDiag templates and ingest pipelines:

```sh
esdiag setup diagnostics-output
```

On Elasticsearch Serverless, `setup` detects the deployment, omits unsupported
ILM settings, and keeps the 30-day data stream retention policy for ordinary
diagnostic data. Diagnostic reports are retained indefinitely. It installs
templates, ingest pipelines, and the bundled `esdiag-user` role. If the setup
credentials cannot manage roles, setup logs a warning and continues; ask a
project administrator to create the role. Serverless security is always
enabled; a `410 Gone` security usage response also reports security as enabled.

Run `esdiag setup <saved-kibana-host>` to install the Kibana assets separately.
On Serverless, the ESDiag space uses the project's solution and feature
visibility. Guided setup skips the stateful Elasticsearch trial-license API;
Agent Builder access depends on the project's feature tier.

ILM, shard, node, and snapshot dashboards remain available for analyzing
diagnostics collected from stateful clusters. Those fields describe the
source cluster, not the Serverless destination. See the
[Serverless asset audit](../reference/serverless-assets.md) for compatibility
details and repeatable verification.

Run this with a credential that can install assets. If normal ingestion uses a
less-privileged credential, replace the saved secret after setup:

```sh
esdiag keystore update diagnostics-output --apikey
esdiag host auth diagnostics-output
```

`setup` prepares Elasticsearch. Use `esdiag init` when you also need the
linked Kibana viewer, dashboards, Agent Builder assets, or a default job.

## Process

Send an archive to the saved output:

```sh
esdiag process /path/to/diagnostic.zip diagnostics-output
```

Or make the destination the default through `esdiag init`, then omit the output:

```sh
esdiag process /path/to/diagnostic.zip
```

If the Kibana viewer and Agent Builder are ready:

```sh
esdiag process /path/to/diagnostic.zip \
  --ask "What is the highest-risk finding, and what evidence supports it?"
```

The result includes a diagnostic ID and a Kibana conversation URL.

## State and maintenance

Saved hosts, jobs, settings, and encrypted secrets live in `~/.esdiag` for the
user who runs `esdiag`. They are separate from full local-stack container state
and native core-mode state. Switching local stack modes does not copy them.

Rotate credentials with:

```sh
esdiag keystore update diagnostics-output --apikey
```

To remove the output, first remove jobs and defaults that reference it:

```sh
esdiag host remove diagnostics-output
esdiag keystore remove diagnostics-output
```

For inference setup, see the [LLM configuration guide](../llm-setup-guide.md).
