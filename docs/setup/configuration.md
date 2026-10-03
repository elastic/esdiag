---
type: Guide
title: Configure ESDiag
description: Set up collection only, a local diagnostic cluster, or a remote cluster.
tags: [setup, onboarding, configuration]
---

# Configure ESDiag

Pick the destination for processed diagnostic documents.

| Destination | Use it when | Setup |
|---|---|---|
| None | You only collect and share raw archives. | `esdiag init`, then choose collection only. |
| Local | You need local Elasticsearch, Kibana, and the ESDiag web UI. | `esdiag local up`, then `esdiag init`. |
| Remote | Your organization already runs the diagnostics cluster. | `esdiag init`, or save a host and run `esdiag setup`. |
| Shared service | An administrator runs the web service and destination. | No local setup for service users. |

Run setup that handles credentials in an interactive terminal. Do not put API
keys, passwords, or upload URLs in chat, tickets, documents, or shell history.

## No diagnostic cluster

Run:

```sh
esdiag init
```

Choose **Only collect diagnostics**. ESDiag saves a collection host and job. It
does not configure an output cluster or install assets.

For the direct command flow, see
[Collect and share diagnostics](local-binary.md).

## Local diagnostic cluster

Start the binary-owned stack:

```sh
esdiag local up
esdiag local auth
esdiag init
```

On a new state directory, `--stack=auto` uses core mode. Core mode runs
Elasticsearch and Kibana in containers and starts native `esdiag serve --mode
user`. Full mode runs the ESDiag web service in a container.

```sh
esdiag local up --stack=core
esdiag local up --stack=full
```

`init` can offer to start a core stack when you select local processing and no
stack exists. Its approval includes the assets needed by that new local
deployment. If you decline, it returns to remote setup without creating local
state. Existing local stacks and remote deployments ask separately before
installing assets.

For a browser-guided first run, choose the web-interface handoff at the start
of `esdiag init`. It opens `/welcome`, where the same workflow is persisted in
stages. The keystore password and endpoint API keys use masked form controls
and are never stored in browser state or `esdiag.yml`.

Submitting each stage updates the current page directly, including when a
diagnostic user was already configured by terminal setup.

At the Diagnostic Source stage, choose **Add later** to open ESDiag without a
collection host or source-dependent default job. Return to `/welcome` when you
are ready to add the source.

When `ESDIAG_OUTPUT_URL` and `ESDIAG_KIBANA_URL` provide the runtime output,
web onboarding treats that deployment as the diagnostic cluster. It validates
both authenticated endpoints and checks the processing and Agent Builder asset
set instead of asking for another cluster. Full `esdiag-local` stacks identify
themselves with `ESDIAG_CONTAINER_LOCAL_STACK=full`, so the page never attempts
to launch containers from inside the ESDiag container.

Core and full modes keep separate ESDiag user state. Switching modes does not
move hosts, jobs, settings, or secrets between the native user directory and
the full-mode container volume.

For the standalone script path:

```sh
esdiag-local up --stack=full
esdiag init
```

See [Run a local diagnostic cluster](esdiag-local.md) for prerequisites and
lifecycle commands.

## Remote diagnostic cluster

For guided setup, run:

```sh
esdiag init
```

Choose processing, then choose **remote**. The initializer saves the output
host, linked Kibana viewer, and encrypted credentials. It also offers to
install output assets.

If validation fails, `init` identifies the endpoint and lets you re-enter its
settings. Declining to replace an existing output returns to output selection
without changing the saved deployment. API key source numbers depend on which
sources are available; use the displayed number or type `paste`, `file`, or `env`.

To configure the destination yourself:

```sh
esdiag keystore unlock
esdiag keystore add diagnostics-output --apikey
esdiag host add diagnostics-output https://diagnostics.example.com:9200 \
  --app elasticsearch --roles send --secret diagnostics-output
esdiag setup diagnostics-output
```

The credential used for `setup` needs permission to install templates and
ingest pipelines. Replace it with a narrower ingest credential afterward if
your organization requires it.

See [Use an existing cluster](existing-cluster.md) for the complete flow.

## Shared service

Service users do not run `init` or `setup`. The service administrator owns the
web URL, authentication, and destination. See
[Use a shared service](shared-service.md).
