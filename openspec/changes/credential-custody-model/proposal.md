## Why

ESDiag's credential handling is specified as a set of storage and unlock *mechanics*
(keystore file, unlock lease, resolution order) without codifying the *model* those
mechanics exist to serve: that credentials divide by stage **direction**, that the app
persists secrets only in `User` mode, and that an unlock grants credential **use**, not
**disclosure**. The two load-bearing security invariants — ad-hoc input keys never leak,
and the unlock file alone never yields usable credentials — live only in prose and are
not expressed as testable spec requirements. Rationale: **ADR-0011**, **ADR-0012**.

## What Changes

- Establish the **credential-direction** model: *input* credentials authenticate to a
  source being `Collect`-ed; *output* credentials authenticate to a `Send`/`Export`/`View`
  destination. The `User`-mode keystore holds saved known-host credentials of **either**
  role; the direction is a property of the referencing host/stage, not of the store.
- Codify the **custody rule**: the app persists secrets **only in `User` mode**. Saved
  known-host credentials persist in the encrypted keystore (`secrets.yml`); *ad-hoc*
  user-provided keys are runtime-only. `Service` mode persists **nothing server-side** —
  output credentials are injected from a vault/secrets service into env vars at container
  runtime, user identity comes from the IAP (ADR-0007), and input keys are ephemeral.
- Add the **ad-hoc input non-leakage invariant** as a testable requirement: an ad-hoc
  input API key on the shared service is one-time-use and MUST never be persisted, logged,
  or included in any event (including the ADR-0008 broadcast/targeted events).
- Add the **use-without-disclosure invariant** for keystore unlock: unlock is a
  time-limited grant that lets ESDiag *use* saved-host credentials so a delegated actor
  (e.g. an LLM agent) can collect/process *through* ESDiag while never reading plaintext.
  The load-bearing property — reading the unlock file (with or without the keystore file)
  MUST NOT by itself yield usable credentials — becomes a spec scenario.
- Record that the **custody backend is an axis independent of runtime mode**, and note an
  OS-native keystore (Keychain / Credential Manager / Secret Service) as a **deferred**
  candidate backend, not implemented here.
- **Not BREAKING:** this codifies existing behavior and invariants; no storage format,
  CLI surface, or endpoint changes. The unlock-file confidentiality requirement is
  strengthened in intent but not in wire format.

## Capabilities

### New Capabilities

- _(none — this modifies existing capabilities)_

### Modified Capabilities

- `host-secret-store`: add credential-direction classification, the `User`-mode-only
  persistence rule (with the `Service`-mode no-server-side-persistence contract), the
  ad-hoc input-key non-leakage invariant, and the backend-vs-mode axis (deferred OS-native
  backend).
- `cli-keystore-lifecycle`: add the unlock use-without-disclosure requirement carrying the
  load-bearing "unlock file alone yields nothing usable" invariant; the same channel-agnostic
  guarantee governs the Web UI unlock path.

## Impact

- **Core processing:** credential resolution and the ad-hoc input path (`Collect`
  receivers) must guarantee non-leakage into logs, reports, and events; the keystore
  unlock/derivation scheme must preserve the disclosure invariant.
- **CLI:** `esdiag keystore unlock`/`lock`/`status` semantics are unchanged; the unlock
  file's confidentiality guarantee is now specified as load-bearing.
- **Web UI:** the shared file-based unlock lease inherits the same use-without-disclosure
  guarantee; `Service`-mode keystore controls remain absent (already specified).
- **Docs:** aligns the specs with ADR-0011 and ADR-0012; part of the architecture-review
  series.
