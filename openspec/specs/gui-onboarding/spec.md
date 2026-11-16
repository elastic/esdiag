# gui-onboarding Specification

## Purpose
Provide a secure, resumable web experience for configuring a user-mode ESDiag
diagnostic workflow from the same non-secret configuration used by the CLI.

## Requirements

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

#### Scenario: Server starts onboarding without an output

- **GIVEN** no output deployment is configured
- **WHEN** the user-mode server starts web onboarding
- **THEN** its structured readiness result reports the output as unconfigured
- **AND** onboarding treats the missing output as expected state without warning on each render

#### Scenario: Diagnostic source is deferred

- **GIVEN** the user selected a workflow that collects diagnostics
- **AND** any required output deployment is configured
- **WHEN** the user chooses to add the diagnostic source later
- **THEN** onboarding preserves the selected workflow and opens the normal application
- **AND** it does not require a source-dependent default job
- **AND** the user can return to onboarding to configure the source

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

#### Scenario: User validates a new diagnostic source

- **WHEN** the user submits a new source and API key
- **THEN** the server validates the endpoint with the submitted key before creating its keystore reference
- **AND** a validation error remains browser text rather than an executable expression

#### Scenario: User validates the first output deployment

- **WHEN** the user submits new Elasticsearch and Kibana output endpoints and an API key
- **THEN** the server validates both endpoints with the submitted key before creating their keystore references
- **AND** successful validation persists the linked hosts and selected output
- **AND** the shared event stream patches the next onboarding stage

### Requirement: Service-Mode Onboarding Boundary

The web interface SHALL not offer persistent local onboarding in service mode.
Service-mode deployment and credentials SHALL remain supplied by its
administrator-controlled runtime configuration.

#### Scenario: Service-mode user opens onboarding route

- **GIVEN** the web interface is running in service mode
- **WHEN** a user requests the onboarding flow
- **THEN** the interface explains that the service administrator owns configuration
- **AND** it does not write local application, host, job, keystore, or settings state

### Requirement: Web Asset Setup Requires Explicit Trial Consent
The web onboarding asset-setup stage SHALL present an unchecked "Start an
Elasticsearch trial license" choice, and SHALL pass it to the shared setup
pipeline as the trial opt-in. It MUST NOT start a trial license unless the
user checks that choice. When the license is inactive and the choice is
unchecked, the stage SHALL install Elasticsearch assets, skip Kibana assets,
and display the licensing warning instead of a Kibana error.

#### Scenario: Default asset setup on a basic license
- **GIVEN** the configured output's Agent Builder license is inactive
- **WHEN** the user runs the asset-setup stage without checking the trial choice
- **THEN** no `_license/start_trial` request is sent
- **AND** the stage reports that Kibana Agent Builder assets need an Enterprise or trial license

#### Scenario: User checks the trial choice
- **GIVEN** the configured output's Agent Builder license is inactive
- **WHEN** the user checks the trial choice and runs the asset-setup stage
- **THEN** a trial license is started before Kibana assets are installed

#### Scenario: Active license
- **GIVEN** the configured output's Agent Builder license is active
- **WHEN** the user runs the asset-setup stage
- **THEN** Elasticsearch and Kibana assets are installed regardless of the trial choice
