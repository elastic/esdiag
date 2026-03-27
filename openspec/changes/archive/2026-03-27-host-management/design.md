## Context

`Commands::Host` currently has only two effective paths: build a new `KnownHost` when both `app` and `url` are provided, or load an existing saved host unchanged when they are not. That means saved-host maintenance flows such as rotating a secret reference, replacing API key auth, or adjusting roles cannot be expressed as partial updates from the CLI even though the command surface already exposes those flags.

This change affects CLI behavior, saved-host merging, and validation/persistence semantics for Elasticsearch, Kibana, and Logstash host records. It also extends CLI lifecycle management to cover deletion of persisted hosts. The implementation must stay self-contained, cross-platform, and compatible with the existing `hosts.yml` plus keystore model.

## Goals / Non-Goals

**Goals:**
- Allow `esdiag host <name>` to update an existing saved host in place when mutable override flags are provided.
- Allow `esdiag host <name> --delete` to remove an existing saved host from CLI-managed storage.
- Preserve the saved host's existing `app`, `url`, and other omitted fields during partial updates.
- Support saved-host updates for auth fields, secret references, role assignments, and certificate validation settings.
- Keep existing full-definition behavior when the caller supplies `app` and `url`.
- Reuse existing validation and connection-test behavior so only valid, reachable merged host records are persisted.

**Non-Goals:**
- Redesign the `KnownHost` file format or keystore storage model.
- Change the web host-management flow.
- Add bulk host editing or multi-record CLI commands.
- Expand host editing beyond the current saved-host fields already modeled by `KnownHost`.

## Decisions

1. **Define four CLI host modes**
   - Decision: Treat `esdiag host` invocations as one of four modes:
     - full definition/create-or-replace when `app` and `url` are supplied
     - delete when `--delete` is supplied
     - incremental update when the named host already exists and one or more mutable overrides are supplied
     - validation-only when the named host exists and no mutable overrides are supplied
   - Rationale: This preserves current explicit create behavior while making update-style invocations do what users expect for existing saved records and provides a CLI-native way to remove saved hosts.
   - Alternatives considered:
     - Require `app` and `url` for every change: rejected because it keeps the current silent no-op problem.
     - Always mutate existing hosts when they exist: rejected because bare `esdiag host <name>` should remain a safe revalidation path.
     - Keep deletion available only in web flows: rejected because the CLI already owns saved-host creation and update semantics.

2. **Merge overrides onto the persisted host before validation**
   - Decision: Load the existing `KnownHost`, convert it into a mergeable representation, apply only the supplied overrides, then rebuild and validate the merged host before saving it.
   - Rationale: Centralized merge logic avoids duplicating per-auth and per-role behavior in `main.rs`, and it preserves omitted fields by default.
   - Alternatives considered:
     - Reconstruct ad hoc per enum variant inside the command handler: rejected because it is brittle and makes auth transitions harder to reason about.
     - Mutate serialized YAML directly: rejected because it bypasses existing model validation and normalization.

3. **Track override intent separately from override values**
   - Decision: Introduce a CLI override representation that records whether each mutable field was supplied, not just its value, so update mode can distinguish omitted fields from requested changes. For certificate validation specifically, treat `--accept-invalid-certs` as an explicit boolean-valued override: omitting the flag preserves the saved value, `--accept-invalid-certs true` enables it, and `--accept-invalid-certs false` clears it.
   - Rationale: Partial updates must preserve existing values unless the user explicitly overrides them. Certificate validation must support both preserving the current setting and explicitly removing a previously enabled override.
   - Alternatives considered:
     - Reuse the current parsed values directly: rejected because defaulted fields collapse "not provided" and "set to false" into the same state.
     - Use paired enable/disable flags: rejected because a single explicit boolean flag is sufficient and keeps the CLI surface smaller.

4. **Keep saved-host updates gated by the existing validation flow**
   - Decision: Run merged host records through the existing `KnownHost` normalization and `validate_host_connection` flow before persisting changes. This applies to every incremental update, including metadata-only changes.
   - Rationale: Update mode should not create a bypass that can save structurally invalid or unreachable host records, and saved-host maintenance should continue to prove the resulting definition is still usable before it is written back to disk.
   - Alternatives considered:
     - Save first and validate later: rejected because it would allow bad partial updates into `hosts.yml`.
     - Skip live validation for metadata-only updates: rejected because even small auth, role, or transport changes can invalidate the effective saved host.

5. **Fail fast for missing-host partial updates**
   - Decision: If the named host does not exist and the invocation does not include the required create fields, return an explicit error instead of inferring a partial record.
   - Rationale: Users need a clear distinction between creating a new host and mutating an existing saved record.
   - Alternatives considered:
     - Auto-create placeholder hosts from partial flags: rejected because it would create invalid and confusing saved records.

6. **Make deletion explicit and mutually exclusive**
   - Decision: Support deletion only through `esdiag host <name> --delete`, and treat that option as mutually exclusive with create and update fields.
   - Rationale: Deletion should be intentional and unambiguous, with no possibility of combining removal with other host mutations.
   - Alternatives considered:
     - Infer deletion from missing fields or empty values: rejected because it is too easy to trigger accidentally.
     - Allow delete plus other overrides: rejected because the resulting behavior would be confusing and hard to document.

## Risks / Trade-offs

- **[Risk] Auth override transitions can accidentally retain stale fields** -> **Mitigation:** Centralize merge rules so each override path rewrites the resulting auth shape explicitly and is covered by CLI tests.
- **[Risk] Certificate validation updates require a CLI syntax adjustment from the current presence-only flag shape** -> **Mitigation:** Parse `--accept-invalid-certs` as an explicit boolean value and add regression tests for omitted, `true`, and `false` cases.
- **[Risk] Existing invalid saved hosts may now fail update attempts once merged validation runs** -> **Mitigation:** Keep validation errors explicit so users can correct the persisted record instead of silently saving a broken configuration.
- **[Risk] Update semantics could diverge from the web host editor over time** -> **Mitigation:** Keep merge and normalization helpers in shared host-model code rather than embedding policy only in the CLI path.
- **[Risk] CLI deletion could leave stale references in other local settings** -> **Mitigation:** Reuse or align with existing saved-host removal helpers so dependent local settings are updated consistently when a host is removed.

## Migration Plan

1. Add CLI command parsing that can distinguish full-definition, delete, incremental-update, and validation-only invocations.
2. Implement merge helpers for existing `KnownHost` records and route update-mode `esdiag host <name>` through them.
3. Implement `--delete` host removal using the same saved-host persistence rules as other local host-management flows.
4. Reuse current host validation and connection testing before persistence for every non-delete mutation.
5. Add regression tests for auth, role, certificate, delete, and missing-host flows.
6. Update user-facing CLI documentation to describe `--accept-invalid-certs true|false` semantics for saved-host updates and `--delete` for saved-host removal.

Rollback strategy:
- Revert the incremental update path and keep the existing full-definition-only behavior for host changes.
- No data migration is required because `hosts.yml` and keystore storage formats remain unchanged.

## Open Questions

None.
