---
name: esdiag
description: Collect or process Elasticsearch, Kibana, and Logstash diagnostics with `esdiag`. Use for collecting live API diagnostics from a cluster, processing support bundle archives or Elastic upload links, sending results to an output cluster, managing saved hosts and encrypted credentials, running saved diagnostic jobs, or hosting the web user interface.
---

# ESDiag

Use this skill to choose and run the right `esdiag` command sequence safely.

Prefer live help output over memory when behavior is unclear:

```sh
esdiag --help
esdiag <command> --help
```

## Command Routing

- Connection management: `esdiag host`
- Credentials and unlock state: `esdiag keystore`
- Asset setup: `esdiag setup`
- Process diagnostics into output docs: `esdiag process`
- Collect fresh API diagnostics: `esdiag collect`
- Saved reusable jobs: `esdiag job`, or `--save-job <NAME>` on compatible `collect`/`process`
- Web/API intake: `esdiag serve`

## Required Checks

Run `esdiag keystore status` before authenticated collection, processing from saved hosts, saved jobs, or host/keystore changes.

If locked, stop and ask the user to unlock with `esdiag keystore unlock` or through the web UI.

```
esdiag keystore status
```

## Workflow Notes

- Configure an output host or `ESDIAG_OUTPUT_*` before processing into Elasticsearch.
- Run `esdiag setup [HOST]` before first ingestion into a cluster.
- Use `--sources <path/to/sources.yml>` for custom or version-specific API endpoint definitions.
- For `process`, resolve output as stdout (`-`), saved host, filesystem target, or `ESDIAG_OUTPUT_*` when omitted. Do not use raw HTTP URLs as output targets unless saved as hosts.
- For `collect`, `<HOST>` must be a saved host with the `collect` role and `<OUTPUT>` must be an existing directory.
- Use metadata flags (`--account`, `--case`, `--opportunity`, `--user`) when reports need context.
- If `process` prints a `Kibana Link: <url>`, present it as a clickable markdown link.

## Saved Jobs

- Use `--save-job <NAME>` on compatible `collect` or `process` invocations to persist the job before execution. Requires a saved known-host collection input; `process` also requires an explicit output. See `references/cli.md` for command-specific details.
- Use `esdiag job list`, `esdiag job run <NAME>`, and `esdiag job delete <NAME>` to manage saved jobs.
- Saved jobs require persisted known hosts and the keystore feature.

## Troubleshooting Rules

- If auth fails, re-check saved host/app/url/auth mode and whether cert validation is required.
- If a saved-host update fails, remember that `esdiag host update <NAME>` re-validates the merged host definition live before saving it.
- If a host should be removed entirely, prefer `esdiag host remove <NAME>` instead of hand-editing `hosts.yml`.
- If output is not where expected, verify `[OUTPUT]` parsing and known-host name collisions with filenames.
- If setup or ingest fails after version changes, rerun `esdiag setup` before retrying `process`.

## References

- Use `references/cli.md` for command syntax, option details, and output resolution rules.
- Use `references/env-vars.md` for all `ESDIAG_*` environment variables and their purpose.
