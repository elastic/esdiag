# Native `esdiag`

Use the native binary for every ESDiag operation. Before commands that read or
change saved hosts, jobs, or keystore state, check the keystore:

```sh
esdiag keystore status
```

If `unlock_active` is false, ask the user to run `esdiag keystore unlock` in
their terminal. Keep credentials out of the conversation.

## Collect

Collect from a saved host into an existing directory:

```sh
esdiag collect <host> <directory>
```

Use `--type minimal|light|standard|support` to choose scope. For repeatable
work, run a saved job:

```sh
esdiag job run <name>
```

Collection needs a saved host with the `collect` role. It does not accept an
ad hoc target URL.

## Share

Upload newly collected diagnostics to Elastic Upload Service:

```sh
esdiag collect <host> <directory> --upload <upload-id>
```

Upload an existing archive directly:

```sh
esdiag upload <archive> <upload-id>
```

Use ESDiag Lite when collecting from an Elasticsearch cluster without a full
ESDiag installation, or process the archive into a configured output
deployment.

## Process

Process an archive, unpacked directory, saved collect host, or Elastic Upload
URL:

```sh
esdiag process <input> [output]
```

An omitted output uses configured `ESDIAG_OUTPUT_*` values. A named output must
be a saved host. Do not pass a raw HTTP URL as output.

Process and ask Agent Builder about the result in one command:

```sh
esdiag process <input> [output] --ask "<question>"
```

`--ask` needs a Kibana-enabled output deployment and cannot be combined with
output `-`.

## Analyze

Ask the configured Agent Builder agent about an existing diagnostic:

```sh
esdiag agent ask "Analyze diagnostic <diagnostic-id>: <question>"
```

Continue a returned conversation:

```sh
esdiag agent ask --conversation <conversation-id> "Explain the highest-risk finding"
```

Use the returned `kibana_url` as the durable handoff. If the command reports
`retry_safe: false`, direct the user to that URL instead of sending the prompt
again.

## Environment variables

The native binary supports these variables. Secret values belong in the user's
terminal, not this conversation.

| Area | Variables |
|---|---|
| State paths | `ESDIAG_HOME`, `ESDIAG_HOSTS`, `ESDIAG_KEYSTORE` |
| Output | `ESDIAG_OUTPUT_URL`, `ESDIAG_OUTPUT_APIKEY`, `ESDIAG_OUTPUT_USERNAME`, `ESDIAG_OUTPUT_PASSWORD`, `ESDIAG_KIBANA_URL`, `ESDIAG_KIBANA_SPACE` |
| Keystore | `ESDIAG_KEYSTORE_PASSWORD` |
| Report metadata | `ESDIAG_USER` |
| Server | `ESDIAG_MODE`, `ESDIAG_PORT` |
| Export tuning | `ESDIAG_ES_BULK_SIZE`, `ESDIAG_ES_BULK_BYTES`, `ESDIAG_ES_WORKERS`, `ESDIAG_OUTPUT_TASK_LIMIT`, `ESDIAG_REQUEST_TIMEOUT_MS`, `ESDIAG_EXPORT_RETRY_MAX`, `ESDIAG_EXPORT_RETRY_INITIAL_MS`, `ESDIAG_EXPORT_RETRY_MAX_MS` |
| Logging | `LOG_LEVEL` |

`ESDIAG_OUTPUT_*` configures `process`, `setup`, and `serve` when a saved
output host is absent. `ESDIAG_KIBANA_URL` is also required for Kibana setup
and Agent Builder with an environment-backed output.
