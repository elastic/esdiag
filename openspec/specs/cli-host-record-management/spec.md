## Purpose

Define the expected CLI behavior for creating, validating, incrementally updating, and deleting saved host records managed by `esdiag host`.

## Requirements

### Requirement: Incremental Saved Host Updates
The system SHALL treat `esdiag host <name>` as an update operation when a saved host named `<name>` exists and the invocation provides one or more mutable override flags. The update flow SHALL reuse the saved host's persisted `app` and `url`, apply the provided overrides, validate the merged host record, and save it only after the host connection test succeeds.

#### Scenario: Update a saved secret reference without restating the host definition
- **GIVEN** a saved host named `prod-es` exists with persisted `app`, `url`, and auth metadata
- **WHEN** the user runs `esdiag host prod-es --secret prod-es-rotated`
- **THEN** the system loads the saved `prod-es` record
- **AND** applies the new secret reference while preserving the saved `app` and `url`
- **AND** validates and connection-tests the merged host record
- **AND** saves the updated `prod-es` record

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

#### Scenario: Enable invalid certificate acceptance on a saved host
- **GIVEN** a saved host named `staging-es` exists with certificate validation disabled
- **WHEN** the user runs `esdiag host staging-es --accept-invalid-certs true`
- **THEN** the system applies the requested certificate validation setting to the saved host
- **AND** preserves the host's existing `app`, `url`, and persisted auth source unless other overrides are supplied

#### Scenario: Omit certificate flag to preserve the saved setting
- **GIVEN** a saved host named `staging-es` exists with `accept_invalid_certs` already enabled
- **WHEN** the user runs `esdiag host staging-es --roles collect,send`
- **THEN** the system preserves the saved certificate validation setting
- **AND** does not clear or rewrite it only because `--accept-invalid-certs` was omitted

#### Scenario: Remove invalid certificate acceptance from a saved host
- **GIVEN** a saved host named `staging-es` exists with `accept_invalid_certs` enabled
- **WHEN** the user runs `esdiag host staging-es --accept-invalid-certs false`
- **THEN** the system disables invalid certificate acceptance for the saved host
- **AND** preserves the host's existing `app`, `url`, and persisted auth source unless other overrides are supplied

### Requirement: Missing Host Update Guardrail
The system SHALL reject incremental update invocations for unknown host names when the command does not include the required fields to create a new host definition.

#### Scenario: Reject partial update for unknown host
- **GIVEN** no saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host prod-es --secret prod-es-rotated`
- **THEN** the command fails with an explicit error indicating that the host does not exist and requires full definition fields to be created
- **AND** the system does not create or save a partial host record

### Requirement: Saved Host Deletion From CLI
The system SHALL support deleting an existing saved host record with `esdiag host <name> --delete`. The delete option SHALL remove the named host from persisted host storage, SHALL fail with an explicit error when the host does not exist, and SHALL be mutually exclusive with create and update fields.

#### Scenario: Delete an existing saved host
- **GIVEN** a saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host prod-es --delete`
- **THEN** the system removes `prod-es` from persisted host storage
- **AND** does not require `app`, `url`, or connection validation for the delete operation

#### Scenario: Reject delete for an unknown host
- **GIVEN** no saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host prod-es --delete`
- **THEN** the command fails with an explicit error indicating that `prod-es` was not found
- **AND** the system leaves persisted host storage unchanged

#### Scenario: Reject conflicting delete arguments
- **GIVEN** a user invokes the host CLI with delete and update fields together
- **WHEN** the user runs `esdiag host prod-es --delete --secret prod-es-rotated`
- **THEN** the command fails with an explicit error indicating that delete cannot be combined with other host mutation fields
- **AND** the system leaves persisted host storage unchanged
