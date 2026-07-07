## ADDED Requirements

### Requirement: Unlock Delegates Use Without Disclosure
Unlocking the keystore SHALL create a time-limited grant that lets ESDiag *use* saved-host
credentials to collect and process, WITHOUT ever exposing the plaintext to the caller. A
delegated actor — automation, or an LLM agent — MAY drive ESDiag to collect and process
through it during the unlock window but MUST NOT be able to read any saved credential in
plaintext. The grant SHALL be rate-limited against unlock-password brute force
(`KeystoreRateLimit`), and the same use-without-disclosure guarantee SHALL apply whether
the unlock password was entered via the CLI or the Web UI.

As a **load-bearing** invariant, reading the unlock file — with or without the keystore
file — MUST NOT by itself yield a usable credential. Any change to the unlock or key
derivation scheme MUST preserve this property.

#### Scenario: Delegated actor uses credentials without reading them
- **GIVEN** the keystore is unlocked with a valid unexpired lease
- **WHEN** a delegated actor drives ESDiag to collect from a saved host during the unlock window
- **THEN** ESDiag performs the credentialed collection on the actor's behalf
- **AND** the actor never receives the saved credential in plaintext

#### Scenario: Unlock file alone yields no usable credential
- **GIVEN** an attacker obtains the `keystore.unlock` file
- **AND** the attacker does not possess the keystore password
- **WHEN** the attacker attempts to reconstruct a saved credential from the unlock file, with or without also possessing the encrypted keystore file
- **THEN** no usable plaintext credential can be derived outside ESDiag's mediated use

#### Scenario: Expired lease revokes delegated use
- **GIVEN** the unlock lease has expired
- **WHEN** a delegated actor attempts to drive a credentialed operation
- **THEN** ESDiag treats the keystore as locked and does not use any saved credential until a new valid password source is supplied

#### Scenario: Unlock brute force is rate limited
- **WHEN** repeated unlock attempts are made with incorrect passwords
- **THEN** the system rate-limits further unlock attempts to contain brute-force guessing of the unlock password
