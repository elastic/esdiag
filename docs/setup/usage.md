---
type: Guide
title: Use ESDiag
description: Collect, share, process, and analyze diagnostic archives.
tags: [setup, usage, onboarding]
---

# Use ESDiag

## Collect and share

Collect an archive from a saved source:

```sh
mkdir -p "$HOME/diagnostics"
esdiag collect source-cluster "$HOME/diagnostics" --type standard
```

Add `--send '<UPLOAD_ID_OR_URL>'` to send the archive to Elastic Upload
Service after collection. ESDiag keeps the local copy.

To send an archive you already have:

```sh
esdiag send /path/to/diagnostic.zip '<UPLOAD_ID_OR_URL>'
```

Treat Elastic Upload Service IDs and URLs as secrets.

## Process and analyze

Process an archive into the configured output:

```sh
esdiag process /path/to/diagnostic.zip
```

Pass a saved `send` host to choose a different destination:

```sh
esdiag process /path/to/diagnostic.zip diagnostics-output
```

If the output has a linked Kibana viewer and Agent Builder is ready, ask about
the diagnostic as part of processing:

```sh
esdiag process /path/to/diagnostic.zip \
  --ask "What is the highest-risk finding, and what evidence supports it?"
```

The result contains the diagnostic ID and a Kibana conversation URL.

## Collect, process, and analyze

Run a saved job after `esdiag init` has created one:

```sh
esdiag job list
esdiag job run <NAME>
```

To keep the raw archive, collect first and pass its reported path to `process`.
ESDiag sends to Elastic Upload Service only when you run `send` or use
`collect --send`.

## Web UI

After `esdiag local up`, open the URL it prints. The default is
`http://127.0.0.1:2501`. Submit an archive and follow the returned Kibana link.

Use `esdiag local status`, `logs`, `restart`, `down`, and `reset --force` to
operate a binary-owned stack. The standalone forms start with `esdiag-local`.

## Coding-agent skill

After a person has run `esdiag init`, the installed skill can run saved jobs,
process archives, and ask Agent Builder questions. It checks the keystore
before secret-backed work. If the keystore is locked, unlock it locally.

The skill must never receive or store credentials in an agent conversation.

## Shared service

Open the URL from your service administrator, sign in, and submit the archive.
The service chooses the destination. It does not expose hosts, jobs, or
credentials to users. See [Use a shared service](shared-service.md).

## Reference

[Command-line interface reference](../command-line.md) has the exact syntax for
your installed version.
