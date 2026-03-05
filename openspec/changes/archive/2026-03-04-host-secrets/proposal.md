## Why

The current `hosts.yml` stores passwords and API keys in plain text, which is not acceptable in stricter environments and creates avoidable risk for credential exposure. We also need role-aware host targeting now so upcoming workflow and UI refactors can safely filter hosts for collect, send, and view operations.

## What Changes

- Add an optional encrypted secret store to hold host authentication values (passwords, API keys, and similar secrets) outside of `hosts.yml`.
- Split host configuration concerns into non-secret host metadata (`hosts.yml`) and a dedicated secrets artifact/file format selected during design based on encryption library capabilities.
- Extend host configuration with role assignments to support at least `collect`, `send`, and `view` targeting.
- Add role validation rules:
  - `collect` is valid for any host and is the default role.
  - `send` is valid only for Elasticsearch hosts.
  - `view` is valid only for Kibana hosts.
- Update host authentication model to add `secret: Option<String>` in `hosts.yml`, where the value is a secret identifier in the secret store.
- Preserve existing plaintext authentication fields for backward compatibility and optional operation in less secure environments (for example local development).

## Capabilities

### New Capabilities
- `host-secret-store`: Encrypt and persist host secrets in a dedicated keystore, with `hosts.yml` referencing stored secret IDs.
- `host-role-targeting`: Define and validate host roles (`collect`, `send`, `view`) for workflow and UI target selection.

### Modified Capabilities
- `collection-execution`: Host selection inputs are role-aware and constrained by validated host role assignments.

## Impact

- `hosts.yml` parsing, validation, and serialization logic in core configuration handling.
- New secret-store file management and encryption/decryption integration.
- Host model, workflow target selection, and validation paths for collect/send/view phases.
- Potential new crypto dependency and key/keystore handling requirements.
- Backward-compatible behavior for existing plaintext host auth fields in developer/legacy environments.
