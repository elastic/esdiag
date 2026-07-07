---
status: accepted
---

# Credential custody: the app persists secrets only in User mode

ESDiag distinguishes **input** credentials (auth to a source being collected) from
**output** credentials (auth to a `Send`/`Export`/`View` destination), and persists
secrets at the application layer **only in `User` mode**. The encrypted keystore
(`secrets.yml`) holds credentials for *saved known hosts* of any role. `Service` mode
persists nothing itself — it delegates all credential custody to the platform.

## The model

- **`User` mode** — the keystore stores saved known-host credentials
  (`SecretEntry { apikey | basic }`) regardless of role: input (`Collect`) and output
  (`Send`/`View`). Ad-hoc, user-provided keys are runtime-only.
- **`Service` mode** — no application keystore:
  - **output** credentials are injected from a vault/secrets service into environment
    variables at container runtime (orchestration owns them);
  - **user identity** is handled by the identity-aware proxy (ADR-0007), not the app;
  - **input** API keys are provided by users at runtime and never stored.

## Consequences

- **The keystore is a `User`-mode-only fixture** — this is a direct consequence of
  the rule, not an independent choice, and it aligns with ADR-0007's tenancy→capability
  bundling (a shared multi-tenant service must not own a shared secret store).
- **"Input credentials are not stored" holds only for ad-hoc keys.** A saved `Collect`
  known host *does* persist its input credential in the `User`-mode keystore; the real
  distinction is saved-vs-ad-hoc, under mode.
- **No secret ever persists *server-side* in `Service` mode** — output via vault/env,
  identity via IAP, input ephemeral — so a compromised container image or config file
  yields no stored credentials. The invariant is specifically about the *shared
  service*; client-side persistence on the user's own device is a separate axis and is
  unconstrained by it.
- **Ad-hoc input API keys are one-time-use with a strict non-leakage invariant:** on
  the shared service an input key is used for a single execution and must never be
  persisted, logged, or included in any event (including the broadcast/targeted events
  of ADR-0008). This is the input-side counterpart to ADR-0012's unlock invariant and
  is the crux of multi-user input-secret handling.
- Credential direction (input/output) maps onto the six-stage model: input =
  `Collect`, output = `Send`/`Export`/`View`.

## Custody backend is a separate axis from mode

*Where* a secret lives (the custody backend) is distinct from *who runs ESDiag* (the
mode). Today's backends are the encrypted file keystore (`User` mode, `secrets.yml`),
vault→env injection (`Service` output), and ephemeral/runtime (input). Because these
have so far lined up with mode, the two were easy to conflate — but they are separate.

An **OS-native keystore** (macOS Keychain, Windows Credential Manager, Linux Secret
Service) is a candidate fourth backend, currently a *possibility, not a requirement*.
It is notable precisely because it is **not bound to mode**: living on the user's
device (CLI host, desktop/Tauri app, or browser credential API), it could persist
*input* keys client-side even against a `Service` backend — without weakening the
server-side invariant above, since the shared service still persists nothing. It is
also a natural replacement for the `secrets.yml` file backend in `User` mode.

If adopted it interacts with ADR-0012: the OS keystore brings its own ACL/unlock, so
ESDiag's unlock-file + rate-limit may be redundant on that backend — but the
"use without disclosure" delegation guarantee shifts to OS process-ACL and must be
re-evaluated, since OS keystores return plaintext to the requesting process. A full
ADR is deferred until the feature firms up.
