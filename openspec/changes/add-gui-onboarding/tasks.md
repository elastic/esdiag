## 1. Shared Onboarding Integration

- [ ] 1.1 Add user-mode server access to the existing onboarding readiness and stage services without duplicating CLI domain logic.
- [ ] 1.2 Define server-side onboarding stage transitions and Datastar patches for identity, output, assets, collection hosts, and default jobs.
- [ ] 1.3 Add secret-safe server actions that accept masked credential submissions without retaining values in browser state.

## 2. User Interface

- [ ] 2.1 Build an accessible user-mode onboarding entry point and stage views using semantic templates and Datastar signals.
- [ ] 2.2 Show validated existing stages for reuse and require explicit confirmation before replacement.
- [ ] 2.3 Add service-mode routing that explains administrator-owned configuration and performs no local writes.

## 3. Settings Migration

- [ ] 3.1 Implement inspection and backup of representable legacy user-mode settings.
- [ ] 3.2 Migrate valid legacy output settings to linked shared application configuration and retain recovery guidance for unsupported values.
- [ ] 3.3 Switch user-mode server startup and settings updates to shared application configuration after successful migration.

## 4. Verification and Documentation

- [ ] 4.1 Add server and UI tests for onboarding stage progression, secret redaction, service-mode exclusion, and settings migration rollback.
- [ ] 4.2 Update user-facing setup and desktop documentation for web onboarding and migration behavior.
- [ ] 4.3 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features`.
- [ ] 4.4 Run `openspec validate add-gui-onboarding --strict`.
