## Why

The web and desktop user experience still uses the legacy `settings.yml`
preferences flow, while CLI initialization now stores reusable non-secret
configuration in `esdiag.yml`. A dedicated GUI onboarding change is needed to
let people configure a complete diagnostic workflow through the UI without
silently changing the current desktop settings behavior.

## What Changes

- Add a user-mode web onboarding flow that guides identity, output deployment,
  credentials, asset setup, collection hosts, and default jobs through the
  existing flow-neutral onboarding services.
- Migrate representable user-mode `settings.yml` values to `esdiag.yml` with a
  backup and explicit recovery behavior.
- Replace the user-mode desktop settings persistence path only after the GUI
  flow can safely manage output configuration.
- Keep service mode environment-driven and without local credentials or
  configuration persistence.

## Capabilities

### New Capabilities

- `gui-onboarding`: Web and desktop onboarding flow for a complete local
  diagnostic workflow.

### Modified Capabilities

- `desktop-settings`: Move user-mode output preference persistence from legacy
  settings to the shared application configuration after safe migration.

## Impact

- Affects the Axum/Datastar settings and onboarding UI, user-mode server
  startup, and desktop wrapper startup.
- Reuses `ApplicationConfig`, `OutputDeployment`, and onboarding services
  introduced by CLI initialization.
- Does not alter service-mode persistence or expose credentials in browser
  state.
