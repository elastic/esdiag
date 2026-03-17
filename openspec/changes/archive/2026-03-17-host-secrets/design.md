## Context

`hosts.yml` currently mixes host metadata and authentication secrets, including plaintext passwords and API keys. This is acceptable for local development but does not meet security requirements for stricter environments. The same host definitions are also being prepared for future workflow and UI selection flows that distinguish collect, process/send, and optional view targets.

The change introduces two cross-cutting concerns in configuration: secure secret storage and role-based host targeting. Both affect parsing, validation, runtime resolution of credentials, and user-facing configuration behavior. The implementation must remain cross-platform and self-contained.

## Goals / Non-Goals

**Goals:**
- Store host credentials in an encrypted keystore rather than plaintext `hosts.yml` values.
- Keep secret storage optional so existing low-security/dev usage continues to work.
- Add host role metadata with validation rules:
  - `collect` default and valid for any host type.
  - `send` valid only for Elasticsearch hosts.
  - `view` valid only for Kibana hosts.
- Preserve backwards compatibility with existing auth fields while enabling migration to `secret` references.
- Prepare config semantics to support role-filtered workflow steps in upcoming UI refactors.

**Non-Goals:**
- Full UI implementation for role-based selection workflows.
- Removing plaintext auth fields in this change.
- Introducing external secret manager dependencies (cloud KMS, Vault, etc.).

## Decisions

1. **Split host config and secret material**
   - Decision: Keep non-secret host config in `hosts.yml` and move secret values to a dedicated encrypted secrets file.
   - Rationale: Least disruptive migration path and clear separation of concerns for secure handling.
   - Alternatives considered:
     - Encrypt fields inline in `hosts.yml`: rejected because mixed concerns complicate editing and validation.
     - Require secrets-only mode immediately: rejected because it would break existing workflows.

2. **Optional secret reference model in `hosts.yml`**
   - Decision: Add `secret: Option<String>` as a secret identifier for each host while retaining legacy auth fields.
   - Rationale: Supports progressive migration and backward compatibility.
   - Alternatives considered:
     - Replace auth fields immediately: rejected because it is a breaking change for existing configs.
     - Multiple secret IDs per auth type: rejected for initial scope; a single secret record per host keeps shape simple.

3. **Encrypted local keystore with pluggable file format**
   - Decision: Select encryption library first, then finalize secrets file format according to library constraints (nonce/metadata framing, key derivation inputs, versioning).
   - Rationale: Avoids locking into a format that conflicts with library APIs or security requirements.
   - Alternatives considered:
     - Fixed YAML/JSON schema before library selection: rejected as high rework risk.

4. **Role model and validation at configuration load time**
   - Decision: Add host `roles` with default `collect` if omitted; enforce host-type constraints during validation.
   - Rationale: Fail fast and provide deterministic runtime behavior for target filtering.
   - Alternatives considered:
     - Validate only during execution: rejected due to delayed, harder-to-debug failures.
     - Infer roles solely from host type: rejected because users need explicit selection intent.

5. **Credential resolution precedence**
   - Decision: Resolve credentials from secret store when `secret` is set; otherwise fall back to existing plaintext fields.
   - Rationale: Predictable behavior with explicit secure opt-in and no break for legacy configs.
   - Alternatives considered:
     - Prefer plaintext over secrets when both exist: rejected because it undermines secure configuration intent.

## Risks / Trade-offs

- **[Risk] Key management ergonomics for local encryption** -> **Mitigation:** Define minimal key bootstrap flow, document defaults for dev, and keep explicit error messages for missing/invalid keys.
- **[Risk] Backward compatibility adds branching complexity** -> **Mitigation:** Centralize credential resolution and validate mutually inconsistent config combinations early.
- **[Risk] Role validation can reject existing configs unexpectedly** -> **Mitigation:** Default missing roles to `collect` and provide migration guidance for `send`/`view`.
- **[Risk] Library-driven file format may evolve** -> **Mitigation:** Include format versioning field from first implementation.

## Migration Plan

1. Add parser support for `secret` and `roles` fields in `hosts.yml` while preserving existing auth fields.
2. Introduce encrypted keystore read/write and secret lookup by `secret_id`.
3. Add config validation for role-to-host-type rules and default role assignment.
4. Implement credential resolution precedence (secret reference first, plaintext fallback).
5. Document migration path: create keystore entries, update `hosts.yml` with `secret` IDs, then remove plaintext fields where desired.

Rollback strategy:
- Disable secret-store usage and rely on existing plaintext fields if needed.
- Retain parser compatibility for previous `hosts.yml` shape.

## Open Questions

- Which encryption library best satisfies cross-platform and self-contained constraints while keeping key management practical?
- Should one `secret_id` map to a structured credential object (username/password/api-key) or a typed union keyed by auth mode?
- How should keystore key material be supplied in CLI and desktop contexts (env var, prompt, OS store integration)?
