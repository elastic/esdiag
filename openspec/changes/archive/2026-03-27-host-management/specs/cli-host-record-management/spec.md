## ADDED Requirements

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
The system SHALL preserve saved host fields that are not explicitly overridden by an incremental CLI update.

#### Scenario: Update roles while preserving auth and transport settings
- **GIVEN** a saved host named `prod-kb` includes a persisted URL, secret reference, and certificate validation setting
- **WHEN** the user runs `esdiag host prod-kb --roles collect,view`
- **THEN** the saved host keeps its existing URL, auth configuration, and certificate validation setting
- **AND** the persisted role set becomes `collect,view`

### Requirement: Mutable Saved Host Override Support
The system SHALL support in-place CLI updates for saved host authentication fields, role assignments, and certificate validation settings without requiring the user to resupply the full host definition. For certificate validation settings, the system SHALL update the saved value only when `--accept-invalid-certs <bool>` is provided, SHALL preserve the saved value when the flag is omitted, SHALL enable invalid certificate acceptance when the value is `true`, and SHALL remove a previously enabled invalid-certificate override when the value is `false`.

#### Scenario: Replace saved secret-backed auth with an API key override
- **GIVEN** a saved host named `prod-es` currently references secret-backed authentication
- **WHEN** the user runs `esdiag host prod-es --apikey new-api-key`
- **THEN** the system saves `prod-es` with API key authentication
- **AND** any persisted secret reference for that host is no longer used as the saved auth source

#### Scenario: Enable invalid certificate acceptance on a saved host
- **GIVEN** a saved host named `staging-es` exists with certificate validation disabled
- **WHEN** the user runs `esdiag host staging-es --accept-invalid-certs true`
- **THEN** the system applies the requested certificate validation setting to the saved host
- **AND** preserves the host's existing `app`, `url`, and auth configuration unless other overrides are supplied

#### Scenario: Omit certificate flag to preserve the saved setting
- **GIVEN** a saved host named `staging-es` exists with `accept_invalid_certs` already enabled
- **WHEN** the user runs `esdiag host staging-es --roles collect,send`
- **THEN** the system preserves the saved certificate validation setting
- **AND** does not clear or rewrite it only because `--accept-invalid-certs` was omitted

#### Scenario: Remove invalid certificate acceptance from a saved host
- **GIVEN** a saved host named `staging-es` exists with `accept_invalid_certs` enabled
- **WHEN** the user runs `esdiag host staging-es --accept-invalid-certs false`
- **THEN** the system disables invalid certificate acceptance for the saved host
- **AND** preserves the host's existing `app`, `url`, and auth configuration unless other overrides are supplied

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
