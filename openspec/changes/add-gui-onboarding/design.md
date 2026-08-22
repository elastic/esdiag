## Context

CLI initialization now owns reusable, non-secret application configuration and
flow-neutral onboarding operations. The current user-mode web and desktop paths
still read and write `settings.yml`; service mode intentionally remains
runtime-configured.

## Goals / Non-Goals

**Goals:**

- Build a user-mode onboarding flow over the existing application
  configuration, output-deployment, and onboarding services.
- Move user-mode output preference persistence to `esdiag.yml` with a safe,
  recoverable migration from representable legacy settings.
- Keep browser-visible state free of credentials.

**Non-Goals:**

- Change service-mode deployment ownership or persistence.
- Reimplement host, job, credential, or setup domain logic in the web layer.
- Migrate non-representable legacy settings silently.

## Decisions

### Use server-driven stages

The server owns stage progression and returns Datastar patches for each stage.
Browser signals represent only non-secret selections and validation messages.
This keeps credential processing in typed server-side onboarding services and
avoids duplicating CLI workflow rules in JavaScript.

### Migrate only representable settings with backup

At user-mode startup, the application evaluates the legacy active target and
Kibana URL. It migrates only values that form a valid linked output deployment,
writes a backup before changing the persistence source, and presents recovery
guidance for anything else. The web flow becomes the writer for shared output
preferences only after that transition succeeds.

### Keep service mode outside the flow

Service mode exposes neither a persistent onboarding route nor migration. Its
administrator-supplied exporter remains authoritative, which avoids creating
local state in a multi-user service deployment.

## Risks / Trade-offs

- [Legacy values do not form a valid linked output] → Preserve the legacy file,
  do not switch persistence, and direct the user through onboarding.
- [Browser state accidentally exposes a credential] → Use password inputs,
  one-way server submission, and tests asserting that patches and signals omit
  secrets.
- [CLI and web stages diverge] → Reuse the existing flow-neutral onboarding
  services and add shared behavior tests.

## Migration Plan

1. Add server-side onboarding stages and service-mode access controls.
2. Add user-mode legacy-settings inspection, backup, and migration.
3. Switch user-mode startup and settings updates to shared configuration after
   successful migration.
4. Retain rollback by restoring the backup and retaining legacy settings reads
   until migration support is proven.
