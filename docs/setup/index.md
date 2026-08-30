---
type: Guide
title: Set up ESDiag
description: Pick an installation, a diagnostic destination, and a way to work.
tags: [setup, onboarding]
---

# Set up ESDiag

Start with the job you need to do.

| Job | Install | Configure | Use |
|---|---|---|---|
| Collect and share a raw archive | [Binary](installation.md#binary) | [No diagnostic cluster](configuration.md#no-diagnostic-cluster) | [Collect and share](usage.md#collect-and-share) |
| Process and analyze an archive | [Binary, launcher, or skill](installation.md) | [Local](configuration.md#local-diagnostic-cluster) or [remote](configuration.md#remote-diagnostic-cluster) | [Process and analyze](usage.md#process-and-analyze) |
| Collect, share, process, and analyze | [Binary or skill](installation.md) | [Local](configuration.md#local-diagnostic-cluster) or [remote](configuration.md#remote-diagnostic-cluster) | [Run the full workflow](usage.md#collect-process-and-analyze) |
| Use an organization-hosted service | [No local install](installation.md#shared-service) | [Administrator-owned](shared-service.md) | [Shared web service](shared-service.md#service-users) |

The ESDiag Skill runs the installed `esdiag` binary. It is not a separate
runtime. A shared-service user only needs the service URL. The administrator
owns that service's exporter, credentials, and upgrades.

## Read these next

1. [Install ESDiag](installation.md)
2. [Configure ESDiag](configuration.md)
3. [Use ESDiag](usage.md)

## Terms used in these guides

- A `collect` host is the system ESDiag reads.
- A `send` host is the Elasticsearch destination for processed documents.
- A `view` host is the linked Kibana endpoint.
- `esdiag init` saves an interactive setup. `esdiag setup` installs or updates
  assets in an output cluster.

Collection does not process or send an archive. Processing does not send
the raw archive. Run the command for each destination you need.
