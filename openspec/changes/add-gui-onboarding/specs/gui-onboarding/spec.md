## Purpose

Provide a secure, resumable web experience for configuring a user-mode ESDiag
diagnostic workflow from the same non-secret configuration used by the CLI.

## ADDED Requirements

### Requirement: User-Mode Web Onboarding

The web interface SHALL offer an onboarding flow when user-mode configuration
is absent or incomplete. It SHALL guide the user through identity, output
deployment, optional asset setup, collection hosts, and a default job according
to the selected workflow. It SHALL reuse validated existing stages by default
and require confirmation before replacement.

#### Scenario: Incomplete local workflow opens onboarding

- **GIVEN** the user-mode web interface has an incomplete application workflow
- **WHEN** the user opens onboarding
- **THEN** it displays the first incomplete applicable stage
- **AND** it preserves already validated configuration references

#### Scenario: Completed workflow opens normal application controls

- **GIVEN** the user-mode application configuration is complete
- **WHEN** the user opens the web interface
- **THEN** it does not require onboarding before normal diagnostic controls are available

### Requirement: Browser Credential Protection

The web onboarding flow SHALL accept credentials only through password-masked
inputs and SHALL submit them only to the server-side keystore operations. It
MUST NOT place secret values in Datastar signals, HTML patches, browser
storage, URLs, application configuration, or logs.

#### Scenario: User enters an output API key

- **WHEN** the user provides an output API key in onboarding
- **THEN** the browser masks the value
- **AND** the server stores it through the encrypted keystore
- **AND** subsequent browser state contains no copy of the key

### Requirement: Service-Mode Onboarding Boundary

The web interface SHALL not offer persistent local onboarding in service mode.
Service-mode deployment and credentials SHALL remain supplied by its
administrator-controlled runtime configuration.

#### Scenario: Service-mode user opens onboarding route

- **GIVEN** the web interface is running in service mode
- **WHEN** a user requests the onboarding flow
- **THEN** the interface explains that the service administrator owns configuration
- **AND** it does not write local application, host, job, keystore, or settings state
