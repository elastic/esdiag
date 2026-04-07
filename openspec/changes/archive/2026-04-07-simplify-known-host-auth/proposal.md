## Why

Saved hosts currently persist an `auth`-tagged enum shape even though effective authentication is already determined primarily by secret references and CLI-supplied overrides. That makes `hosts.yml` harder to evolve, couples persistence to runtime auth branching, and obscures the intended direction toward secret-backed host records while still needing compatibility for existing saved hosts.

## What Changes

- Refactor saved host persistence so new `hosts.yml` records no longer serialize the `auth` tag or auth-shaped enum variants.
- Preserve full read compatibility for legacy host records that still use tagged `ApiKey`, `Basic`, or `NoAuth` formats and may include inline plaintext credentials.
- Treat secret references as the canonical persisted host auth source in the new format, while allowing hosts without a secret reference to persist only when the host truly validates as no-auth.
- Preserve CLI-provided authentication for supported non-persisting flows, while requiring a persisted secret reference for saved hosts that need authentication.
- Keep `esdiag keystore migrate` fully functional for legacy hosts by continuing to detect, upgrade, and rewrite legacy inline credentials into secret-backed host references.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `cli-host-record-management`: saved host creation and update flows will write a flat host record format without the `auth` tag while still supporting CLI-provided auth for validation and compatible update paths.
- `host-secret-store`: host auth resolution and migration behavior will distinguish legacy inline auth compatibility from the new persisted secret-backed-or-noauth host model.

## Impact

- Affected code: `src/data/known_host.rs`, `src/data/uri.rs`, CLI host handling in `src/main.rs`, server host management views/handlers, and host-related tests.
- Affected storage: `hosts.yml` write format changes for newly written host records, while legacy files remain readable.
- Affected commands: `esdiag host`, host-backed runtime resolution, and `esdiag keystore migrate`.
