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

The output needs ESDiag templates and ingest pipelines:

```sh
esdiag setup diagnostics-output
```

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
