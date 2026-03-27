## Why

The `esdiag host` CLI can create and validate saved hosts, but it does not support partial updates for existing records and does not currently provide a CLI path to delete a saved host. For saved Elasticsearch, Kibana, and Logstash hosts, update-style invocations such as `esdiag host <name> --secret <secret-id>` silently preserve the old record, which makes routine host maintenance unreliable and leaves deletion available only through non-CLI flows.

## What Changes

- Treat `esdiag host <name>` as an update operation when `<name>` already exists and the invocation provides mutable host flags.
- Reuse the saved host's persisted `app` and `url` when applying CLI updates so users do not need to restate the full host definition for common changes.
- Support in-place updates for saved host authentication, role assignments, and certificate validation settings, then validate and connection-test the merged record before saving it.
- Add `esdiag host <name> --delete` to remove an existing saved host record from the CLI.
- Preserve current behavior for full host creation or replacement when `app` and `url` are supplied explicitly.
- Keep CLI errors explicit when the named host does not exist and the command does not include the required fields to create a new record.

## Capabilities

### New Capabilities
- `cli-host-record-management`: Create, validate, incrementally update, and delete saved host records from the CLI.

### Modified Capabilities
None.

## Impact

- CLI host argument parsing and command execution in `src/main.rs`.
- Saved host merge, normalization, and persistence helpers in `src/data/known_host.rs`.
- CLI regression coverage for saved-host update and delete flows in `tests/`.
- User-facing host management behavior for saved Elasticsearch, Kibana, and Logstash records.
