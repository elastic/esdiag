---
type: Reference
title: "Keystore unlock delegates time-limited credential *use*, not disclosure"
status: accepted
tags: [repository, adr]
---

# Keystore unlock delegates time-limited credential *use*, not disclosure

Unlocking the keystore (by password, via CLI or Web UI) creates a **time-limited
unlock** that lets ESDiag *use* saved-host credentials to collect and process,
**without ever exposing the plaintext to the caller**. The design goal is delegation:
an automated actor — notably an LLM agent — can operate on the user's behalf during
the unlock window but can never read credentials out of the encrypted keystore. Use
is delegated; the secret is not.

## Considered options

- **In-memory session only.** Rejected: lost on restart and cannot span a separate,
  delegated invocation (e.g. an agent process), so it cannot support automated use.
- **Hand the decrypted credential to the caller.** Rejected: exposes the secret to
  the (possibly untrusted) delegated actor, defeating the entire point.
- **Time-limited on-disk unlock granting mediated use (chosen).** ESDiag performs the
  credentialed operations; the caller drives ESDiag but never sees the secret.

## Consequences

- **Channel-agnostic** — the same unlock applies whether the password was entered via
  CLI or Web UI.
- **Rate-limited** (`KeystoreRateLimit`) against unlock-password brute force.
- **Blast-radius containment** — a compromised delegated actor can trigger ESDiag
  operations during the window, but cannot exfiltrate the credential for reuse
  elsewhere.
- **Load-bearing invariant:** the property holds *only if the unlock file does not let
  a caller reconstruct the plaintext outside ESDiag's mediation* — i.e. reading the
  unlock file (with or without the keystore file) must not by itself yield usable
  credentials. This is the security crux and must be preserved by any change to the
  unlock/derivation scheme.
- Applies only in `User` mode — `Service` mode has no keystore (ADR-0011).
