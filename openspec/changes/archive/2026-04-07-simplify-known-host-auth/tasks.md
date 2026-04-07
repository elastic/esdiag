## 1. Saved Host Model Refactor

- [x] 1.1 Replace the tagged `KnownHost` persistence model with a flat saved-host representation that no longer serializes the `auth` tag.
- [x] 1.2 Refactor host helper methods (`app`, `url`, roles, viewer, cloud routing, certificate settings) to work without enum-variant auth branching.
- [x] 1.3 Update URI conversion and cloud host classification to rely on explicit host fields rather than `ApiKey` variant identity.

## 2. Legacy Compatibility And CLI Auth

- [x] 2.1 Add backward-compatible deserialization for legacy tagged host records and legacy inline auth fields.
- [x] 2.2 Preserve CLI-provided auth for supported non-persisting and compatibility-sensitive flows while preventing the new saved format from persisting inline auth state.
- [x] 2.3 Update host save/update rewrite behavior so secret-backed hosts persist a `secret` reference, true no-auth hosts persist without a secret, and authenticated hosts cannot be saved without a secret reference.

## 3. Migration And Verification

- [x] 3.1 Preserve and verify `esdiag keystore migrate` behavior for legacy API key and basic-auth hosts, including rewriting migrated hosts in the new format.
- [x] 3.2 Update server/UI and host-management tests that assume `apikey|basic|none` saved variants or `auth`-tagged serialization.
- [x] 3.3 Run `cargo clippy`.
- [x] 3.4 Run `cargo test`.
