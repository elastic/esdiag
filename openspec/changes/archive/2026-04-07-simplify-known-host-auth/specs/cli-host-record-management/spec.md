## MODIFIED Requirements

### Requirement: Partial Override Preservation
The system SHALL preserve saved host fields that are not explicitly overridden by an incremental CLI update. In the flat saved-host format, preserved fields SHALL include the saved host's `app`, `url`, `roles`, `viewer`, `cloud_id`, `accept_invalid_certs`, and persisted `secret` reference. The system SHALL NOT infer or persist a saved auth tag or saved inline auth mode solely from omitted fields or transient CLI auth provided for the current command.

#### Scenario: Update roles while preserving a saved secret reference
- **GIVEN** a saved host named `prod-kb` includes a persisted URL, secret reference, and certificate validation setting
- **WHEN** the user runs `esdiag host prod-kb --roles collect,view`
- **THEN** the saved host keeps its existing URL, secret reference, and certificate validation setting
- **AND** the persisted role set becomes `collect,view`

### Requirement: Mutable Saved Host Override Support
The system SHALL support in-place CLI updates for saved host secret references, role assignments, and certificate validation settings without requiring the user to resupply the full host definition. The system SHALL accept CLI-provided auth inputs for supported non-persisting or compatibility-sensitive flows, but SHALL persist only secret references as durable saved auth state for authenticated saved hosts in the new host format. For certificate validation settings, the system SHALL update the saved value only when `--accept-invalid-certs <bool>` is provided, SHALL preserve the saved value when the flag is omitted, SHALL enable invalid certificate acceptance when the value is `true`, and SHALL remove a previously enabled invalid-certificate override when the value is `false`.

#### Scenario: Save a host with a secret-backed auth source
- **GIVEN** a user creates or updates a saved host and provides `--secret prod-es-main`
- **WHEN** the command succeeds and writes the saved host record
- **THEN** the persisted host record stores `secret: prod-es-main`
- **AND** the persisted host record does not require an `auth` tag to describe the saved auth source

#### Scenario: Validate a host with transient CLI auth without persisting a saved host record
- **GIVEN** a user invokes `esdiag host prod-es ... --apikey transient-key --nosave`
- **WHEN** the command validates the host connection successfully
- **THEN** the system uses the CLI-provided API key for that command
- **AND** the system does not persist a saved host record with inline auth or inferred saved auth mode

#### Scenario: Reject saving an authenticated host without a secret reference
- **GIVEN** a host endpoint requires authentication to validate successfully
- **AND** the user provides transient CLI auth without a `--secret` reference
- **WHEN** the user attempts to save the host record
- **THEN** the command fails with an explicit error indicating that saved authenticated hosts require a secret reference
- **AND** the system does not persist the host as a no-auth record

#### Scenario: Remove invalid certificate acceptance from a saved host
- **GIVEN** a saved host named `staging-es` exists with `accept_invalid_certs` enabled
- **WHEN** the user runs `esdiag host staging-es --accept-invalid-certs false`
- **THEN** the system disables invalid certificate acceptance for the saved host
- **AND** preserves the host's existing `app`, `url`, and persisted auth source unless other overrides are supplied
