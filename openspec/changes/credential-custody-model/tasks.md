# Tasks

## 1. Credential direction model
- [x] 1.1 Introduce (or confirm) a `CredentialDirection` (`Input` | `Output`) derived from the referencing host/stage — `Collect` ⇒ input; `Send`/`Export`/`View` ⇒ output — and thread it where credentials are resolved.
- [x] 1.2 Confirm the keystore stores saved known-host credentials of either direction with no store-level direction attribute.
- [x] 1.3 Ensure display/telemetry reference direction via the host/stage, never a stored field.

## 2. Custody-by-mode rule
- [x] 2.1 Assert that credential persistence to the keystore occurs only in `User` mode; ad-hoc keys are runtime-only in any mode.
- [x] 2.2 In `Service` mode, resolve output credentials from runtime-injected environment variables (vault/secrets service) and never write them to an application store.
- [x] 2.3 Confirm `Service`-mode input keys are held only for the execution and never persisted server-side.
- [x] 2.4 Add a check/test that a `Service`-mode image + config contains no persisted credential.

## 3. Ad-hoc input non-leakage invariant
- [x] 3.1 Audit the ad-hoc `Collect` input path for the shared service: no keystore/host-record/disk write of the key.
- [x] 3.2 Ensure the input key is redacted from all log levels (add redaction at the boundary if absent).
- [x] 3.3 Ensure no event payload (including ADR-0008 broadcast/targeted events) can carry the input key or a reconstructable form.
- [x] 3.4 Ensure the ad-hoc key does not outlive the execution that consumed it.

## 4. Unlock use-without-disclosure invariant
- [x] 4.1 Confirm the unlock grant enables mediated *use* of saved credentials but never returns plaintext to the caller (CLI or Web UI).
- [x] 4.2 Verify the load-bearing property: the unlock file alone (with or without the keystore file) does not yield a usable credential; document the derivation dependency that guarantees it.
- [x] 4.3 Confirm expired lease revokes delegated use and that unlock is rate-limited (`KeystoreRateLimit`).

## 5. Backend-vs-mode axis
- [x] 5.1 Ensure backend selection is driven by direction/configuration, not by mode as a proxy.
- [x] 5.2 Confirm no OS-native keystore backend is exposed (deferred); leave a code/spec note referencing ADR-0011/ADR-0012 for the future ADR.

## 6. Verification
- [x] 6.1 Test: saved collect host resolves an input credential; saved destination host resolves an output credential; keystore record carries no direction.
- [x] 6.2 Test: `User` mode persists saved-host credentials; ad-hoc keys are not persisted.
- [x] 6.3 Test: `Service` mode persists nothing server-side (output via env, input ephemeral).
- [x] 6.4 Test: ad-hoc input key never appears in store, logs, or any event, and does not survive its execution.
- [x] 6.5 Test: delegated actor collects through ESDiag during unlock without receiving plaintext; unlock file alone yields nothing usable; expired lease locks; brute force is rate-limited.
- [x] 6.6 Confirm the delta spec scenarios in `specs/host-secret-store/spec.md` and `specs/cli-keystore-lifecycle/spec.md` are covered.
