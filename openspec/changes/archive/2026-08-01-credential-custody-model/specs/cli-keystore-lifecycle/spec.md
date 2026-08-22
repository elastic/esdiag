## ADDED Requirements

### Requirement: Unlock Delegates Use Without Disclosure
Unlocking the keystore SHALL create a time-limited grant that lets ESDiag *use* saved-host
credentials to collect and process, WITHOUT ever exposing the plaintext to the caller. A
delegated actor — automation, or an LLM agent — MAY drive ESDiag to collect and process
through it during the unlock window but MUST NOT be able to read any saved credential in
plaintext. The grant SHALL be rate-limited against unlock-password brute force
(`KeystoreRateLimit`), and the same use-without-disclosure guarantee SHALL apply whether
the unlock password was entered via the CLI or the Web UI.

As a **load-bearing** invariant, no ESDiag interface SHALL return a saved credential in
plaintext, and credential material SHALL be held in a wrapper whose debug and display
forms render a redaction marker and whose serialization is opt-in per field, so that a
new log line or event field cannot disclose a secret by accident. Any change to unlock,
key derivation, or the credential-carrying types MUST preserve this property.

The unlock envelope is encrypted under a key derived from machine context rather than
from the keystore password, because unattended use gives ESDiag no secret to decrypt
with. The guarantee that follows is scoped accordingly: the file holds no plaintext at
rest and does not decrypt under a different machine context, so exfiltrating the bytes
alone yields nothing. Code running on the host as the owning user is explicitly **out
of scope** — it can reconstruct the context — and local file permissions bound that
case instead (ADR-0012).

#### Scenario: Delegated actor uses credentials without reading them
- **GIVEN** the keystore is unlocked with a valid unexpired lease
- **WHEN** a delegated actor drives ESDiag to collect from a saved host during the unlock window
- **THEN** ESDiag performs the credentialed collection on the actor's behalf
- **AND** the actor never receives the saved credential in plaintext

#### Scenario: Exfiltrated unlock file yields no usable credential
- **GIVEN** an attacker obtains the `keystore.unlock` file, and optionally the encrypted keystore file
- **AND** the attacker does not possess the keystore password or the originating machine context
- **WHEN** the attacker attempts to reconstruct a saved credential from those files
- **THEN** the unlock envelope does not decrypt and no plaintext credential can be derived

#### Scenario: Credential material is redacted in debug and log output
- **GIVEN** any type that carries an API key, a password, or the cached keystore password
- **WHEN** a value of that type is formatted for a log line, an error, or an event
- **THEN** the rendered output contains a redaction marker rather than the credential

#### Scenario: Expired lease revokes delegated use
- **GIVEN** the unlock lease has expired
- **WHEN** a delegated actor attempts to drive a credentialed operation
- **THEN** ESDiag treats the keystore as locked and does not use any saved credential until a new valid password source is supplied

#### Scenario: Unlock brute force is rate limited
- **WHEN** repeated unlock attempts are made with incorrect passwords
- **THEN** the system rate-limits further unlock attempts to contain brute-force guessing of the unlock password
