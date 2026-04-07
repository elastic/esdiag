## Context

`KnownHost` currently uses a serde-tagged enum with `ApiKey`, `Basic`, and `NoAuth` variants, and that tagged shape is written directly to `hosts.yml`. Over time, the effective runtime auth source has shifted: when a host references a secret, auth is resolved from the keystore rather than from the saved host variant, and CLI host commands already treat auth inputs as mutable overrides that can be merged and validated separately from the saved host definition.

This leaves the saved data model carrying two overlapping concepts:
- persisted host identity and transport metadata (`app`, `url`, `roles`, `viewer`, certificate settings, cloud routing)
- persisted or legacy auth representation (`auth` tag plus inline `apikey` or `username` / `password`)

The refactor needs to simplify the persisted model without breaking older `hosts.yml` files, without removing CLI-driven auth workflows, and without regressing the existing keystore migration path that upgrades legacy inline credentials into secret references.

## Goals / Non-Goals

**Goals:**
- Remove the `auth` tag from newly written saved host records.
- Make the new persisted host format represent either a secret-backed host or a no-auth host.
- Continue reading legacy tagged host records, including inline plaintext auth, without manual migration steps.
- Preserve CLI-provided auth for supported non-persisting and compatibility-sensitive flows.
- Preserve full `keystore migrate` support for legacy saved hosts.

**Non-Goals:**
- Implement Elastic Cloud cookie-based API auth or other unrelated host transport changes.
- Remove legacy compatibility reads in the same change.
- Redesign the keystore data model or secret record format.
- Change host role validation, viewer resolution, or cloud host routing semantics beyond what is needed to detach them from the `auth` enum variant.

## Decisions

### 1. Persist a flat saved-host record and deserialize both new and legacy shapes

`KnownHost` will move away from a tagged enum as the persisted contract. The new serialized form will be a flat host record containing transport and routing fields such as `app`, `url`, `roles`, `viewer`, `accept_invalid_certs`, `cloud_id`, and optional `secret`.

To preserve compatibility, deserialization will accept:
- the new flat record format
- legacy tagged `auth: ApiKey|Basic|NoAuth` records
- legacy inline auth fields when present

Rationale:
- The persisted contract should represent durable host identity and routing, not runtime auth branching.
- Backward-compatible reads allow existing users to continue operating and migrating without editing `hosts.yml` by hand.
- The change can be rolled out without a file migration step on startup.

Alternatives considered:
- Keep the enum and only hide the `auth` field during serialization. Rejected because the in-memory model would still encode persistence around a runtime auth concept we no longer want to preserve.
- Require users to migrate all hosts before upgrading. Rejected because it would violate the legacy-read constraint and create avoidable operational friction.

### 2. Treat legacy inline auth as compatibility data, not new persisted state

Legacy plaintext auth loaded from older `hosts.yml` entries will remain readable and usable for compatibility-sensitive flows, including host validation and `keystore migrate`. However, newly written host records will not persist inline auth fields in the new format.

This requires separating:
- canonical persisted state: secret-backed auth reference or no auth
- compatibility-only loaded state: legacy `apikey`, `username`, and `password` fields

Rationale:
- This preserves upgrade compatibility and migration support without continuing to endorse inline auth as the steady-state saved format.
- `keystore migrate` still needs access to legacy inline credentials after parsing existing host files.

Alternatives considered:
- Drop inline auth support entirely once the new format lands. Rejected because it would break legacy hosts before migration.
- Continue writing inline auth fields in the new format. Rejected because it would defeat the goal of simplifying and de-emphasizing saved host auth state.

### 3. Restrict transient CLI auth to flows that do not persist an authenticated host

CLI-supplied `--apikey`, `--user`, and `--password` values will remain supported where the command can use them transiently, but a successfully saved host that requires authentication must persist a secret reference rather than being rewritten as a no-auth record.

In practice:
- CLI-provided auth may still be used for non-persisting flows such as `--nosave` validation or other compatibility-sensitive command paths.
- If a host is going to be persisted and requires authentication, the saved record must include a secret reference.
- A saved host record without a secret reference is valid only when the endpoint succeeds as a true no-auth host.

Rationale:
- This preserves supported CLI workflows without introducing an invalid state where an authenticated host is saved as no-auth.
- It keeps the persisted model aligned with actual runtime requirements: saved authenticated hosts remain secret-backed, while no-auth hosts remain genuinely no-auth.

Alternatives considered:
- Remove CLI auth flags from host commands. Rejected because the user explicitly wants CLI-provided auth to remain supported where allowed.
- Save authenticated hosts without a secret reference after transient validation. Rejected because it would produce persisted records that cannot succeed on later reuse.

## Risks / Trade-offs

- **[Risk] Legacy compatibility code may linger and add complexity** -> **Mitigation:** isolate legacy parsing and compatibility auth access behind dedicated helpers so the flat host model remains the primary path.
- **[Risk] Users may expect CLI-provided auth to keep working for saved authenticated hosts without a secret** -> **Mitigation:** make the save semantics explicit: transient CLI auth can validate a command, but persisted authenticated hosts must reference a secret.
- **[Risk] Cloud URI routing currently depends on `ApiKey` variant matching** -> **Mitigation:** move cloud routing decisions to explicit fields such as `cloud_id` rather than enum variant identity.
- **[Risk] Server/UI code may still assume `apikey|basic|none` as a rendered host property** -> **Mitigation:** provide derived helper methods for display and compatibility instead of exposing storage shape directly.

## Migration Plan

1. Introduce the new flat saved-host representation and compatibility deserializer for legacy tagged host records.
2. Refactor host helpers, URI conversion, and CLI merge logic to stop relying on enum variant identity for persisted auth mode.
3. Preserve legacy inline auth access for compatibility and migration-only flows.
4. Update `esdiag host` save/update behavior so authenticated saved hosts require a persisted secret reference while preserving supported transient validation flows.
5. Verify `keystore migrate` still upgrades legacy hosts into secret-backed records and rewrites them in the new format.

Rollback strategy:
- Revert to the tagged enum serializer/deserializer. Because legacy reads remain supported throughout the change, no irreversible data migration is required.

## Open Questions

- Whether the CLI should fail immediately or emit an explicit guided error when a user attempts to save an authenticated host without a secret reference in the new model.
