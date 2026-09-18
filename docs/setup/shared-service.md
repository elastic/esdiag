---
type: Guide
title: Use a shared ESDiag service
description: Upload diagnostics to an organization-managed ESDiag web service.
tags: [setup, service, web, onboarding]
---

# Use a shared ESDiag service

A service user uploads diagnostics through a web URL. A service administrator
runs the service and its output cluster. They have different jobs.

## Service users

1. Get the service URL and data-handling rules from the administrator.
2. Open the URL and sign in.
3. Upload the approved diagnostic archive.
4. Open the returned diagnostic or Kibana link, if the service provides one.

You do not need `esdiag`, `esdiag-local`, or the coding-agent skill. A shared
service does not let users select an output cluster, save credentials, or create
jobs. Contact the administrator if an upload fails or reaches the wrong place.

Shared services do not accept Elasticsearch URLs or API keys through the web UI
or HTTP API. Run ESDiag locally in `user` mode when direct API-key collection is
required. Prefer a saved host with its credential in the encrypted keystore.

## Service administrators

Run one fixed exporter in service mode:

```sh
ESDIAG_MODE=service \
ESDIAG_OUTPUT_URL=https://diagnostics.example.com:9200 \
ESDIAG_KIBANA_URL=https://kibana.example.com/s/esdiag \
esdiag serve
```

Provide `ESDIAG_OUTPUT_*` credentials through your deployment's secret manager.
Do not put them in a user keystore or command history.

Before you publish the URL:

1. Run `esdiag setup` against the output cluster with a credential that can
   install assets.
2. Put the service behind your identity-aware proxy. Set `--identity-header`
   when it does not use `X-Goog-Authenticated-User-Email`. The proxy must remove
   client-supplied values before it sets the trusted identity header.
3. Set the global and per-owner job caps.
4. Publish the URL, permitted data, retention policy, and support contact.

Use `--auth-provider none` only for controlled local testing. This accepts
requests without an identity header and assigns them to the anonymous user.

Service mode always uses the startup exporter. It does not read or write user
hosts, jobs, or keystore state. It also omits user-mode configuration pages.

See [Command-line interface reference](../command-line.md#serve) for the full
set of service options.
