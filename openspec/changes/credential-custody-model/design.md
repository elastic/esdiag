# Design

Full rationale, rejected alternatives, and consequences live in
**`docs/adr/0011-credential-custody-keystore-user-mode-only.md`** and
**`docs/adr/0012-keystore-unlock-delegates-use-not-disclosure.md`**; this design covers
only the approach and the invariants the delta specs must enforce. It is part of the
architecture-review series and builds on the `Platform`/`Application` split (ADR-0001)
and the tenancy→capability bundling of `User`/`Service` mode (ADR-0007).

## Context

Credential behavior today is specified as mechanics — an encrypted keystore
(`host-secret-store`), a file-based unlock lease shared by CLI and Web UI
(`cli-keystore-lifecycle`, `web-keychain-session-unlock`), and a processing preflight
(`web-secure-processing-gate`). What is missing at the spec level is the *model* that
makes those mechanics correct and the two security invariants they exist to uphold. This
change adds no new mechanism; it names the model and pins the invariants as testable
requirements.

## Approach

Two orthogonal classifications, plus the mode-based custody rule, are stated as
requirements on the existing capabilities:

- **Direction** (ADR-0011). A credential is *input* (authenticates to a `Collect`
  source) or *output* (authenticates to a `Send`/`Export`/`View` destination). The
  keystore is role-agnostic: a saved known host persists its credential regardless of
  direction. Direction is derived from the referencing host/stage, never stored as a
  store-level attribute.
- **Custody by mode** (ADR-0011). The app persists secrets **only in `User` mode**.
  Saved known-host credentials → encrypted keystore (`secrets.yml`). Ad-hoc,
  user-provided keys → runtime-only, any mode. `Service` mode persists nothing at the app
  layer: output from vault→env injection (orchestration owns it), identity from the IAP,
  input ephemeral. The real distinction is *saved-vs-ad-hoc under mode* — "input is never
  stored" holds only for ad-hoc keys, since a saved `Collect` host does persist its input
  credential in `User` mode.
- **Backend ≠ mode** (ADR-0011). *Where* a secret lives (backend) is a separate axis from
  *who runs ESDiag* (mode). Today's backends — encrypted file (`User`), vault→env
  (`Service` output), ephemeral (input) — have lined up with mode, but are not the same
  axis. See *Deferred* below.

## Invariants (the crux — expressed as testable scenarios in the deltas)

- **Ad-hoc input non-leakage** (ADR-0011). On the shared service, an ad-hoc input API key
  is used for a single execution and MUST NEVER be persisted, logged, or included in any
  event — including the broadcast/targeted events of ADR-0008. This is the input-side
  counterpart to the unlock invariant and the crux of multi-user input-secret handling.
- **No server-side persistence** (ADR-0011). The `Service`-mode invariant is specifically
  about the *shared service*: a compromised container image or config file yields no
  stored credentials. Client-side persistence on the user's own device is a *separate
  axis* and is unconstrained by this invariant.
- **Use without disclosure** (ADR-0012). An unlock is a time-limited grant of credential
  *use*, not *disclosure*: ESDiag performs the credentialed operation; a delegated actor
  drives ESDiag but never sees the secret. The **load-bearing** property is that reading
  the unlock file — with or without the keystore file — MUST NOT by itself yield usable
  credentials. Any change to the unlock/derivation scheme must preserve this. The grant is
  rate-limited (`KeystoreRateLimit`) against unlock-password brute force, and is
  channel-agnostic (the same guarantee governs CLI and Web UI unlock).

## Risks

- **Invariant drift.** The non-leakage and disclosure properties are easy to erode via a
  well-meaning debug log, a new event field, or a "convenience" unlock scheme. Mitigated
  by making both testable scenarios that a regression must break.
- **Direction ambiguity.** Because the keystore stores both roles, tooling must resolve
  direction from the host/stage, not the store. Mitigated by stating direction as a
  derived property.
- **Backend/mode conflation.** The two axes have historically coincided; new backend work
  must not re-bind them.

## Deferred

An **OS-native keystore** (macOS Keychain, Windows Credential Manager, Linux Secret
Service) is a candidate fourth backend — a *possibility, not a requirement*, and **not
implemented here**. It is notable because it is not bound to mode: living on the user's
device it could persist *input* keys client-side even against a `Service` backend without
weakening the server-side invariant. If adopted it interacts with ADR-0012 — the OS
keystore brings its own ACL/unlock, returning plaintext to the requesting process, so the
use-without-disclosure guarantee would shift to OS process-ACL and must be re-evaluated. A
full ADR is deferred until the feature firms up.
