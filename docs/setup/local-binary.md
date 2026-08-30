---
type: Guide
title: Collect and share diagnostics
description: Collect a raw diagnostic archive and optionally send it to Elastic Upload Service.
tags: [setup, collection, local]
---

# Collect and share diagnostics

This path needs no diagnostic cluster. It creates a local ZIP archive and can
send that archive to Elastic Upload Service.

Install `esdiag` first. See [Install ESDiag](installation.md#binary).

## What you need

- Network access to the Elasticsearch, Kibana, or Logstash endpoint.
- Credentials that can read its diagnostic APIs.
- A local directory with enough space for the archive.

Enter credentials only at ESDiag's masked prompts. Do not put them in shell
history, tickets, documents, or chat.

## Save the source

Create or unlock the encrypted keystore:

```sh
esdiag keystore unlock
```

Add the source credential. Omit the API key or password value so ESDiag prompts
for it:

```sh
esdiag keystore add source-cluster --apikey
```

```sh
esdiag keystore add source-cluster --user elastic --password
```

Save and test the source:

```sh
esdiag host add source-cluster https://es.example.com:9200 \
  --app elasticsearch --roles collect --secret source-cluster
esdiag host auth source-cluster
```

Use the real endpoint, including its base path. Keep certificate verification
enabled unless the endpoint owner approves an exception.

For Kibana or Logstash, change `--app` and the endpoint.

## Collect

The output directory must exist:

```sh
mkdir -p "$HOME/diagnostics"
esdiag collect source-cluster "$HOME/diagnostics" --type standard
```

The command reports the archive path. Check that the file exists before you
share it.

`minimal`, `light`, `standard`, and `support` are the collection levels.
`standard` is the default.

## Send

To send an existing archive to Elastic Upload Service:

```sh
esdiag send /path/to/diagnostic.zip '<UPLOAD_ID_OR_URL>'
```

To send immediately after collection:

```sh
esdiag collect source-cluster "$HOME/diagnostics" \
  --type standard \
  --send '<UPLOAD_ID_OR_URL>'
```

The collected archive stays on disk. Elastic Upload Service IDs and URLs are
sensitive. Use the approved way to prevent them landing in shell history.

## Next

To process the archive, configure a
[local](configuration.md#local-diagnostic-cluster) or
[remote](configuration.md#remote-diagnostic-cluster) diagnostic cluster.
