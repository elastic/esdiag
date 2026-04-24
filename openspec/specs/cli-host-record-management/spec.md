## Purpose

Define the expected CLI behavior for creating, validating, incrementally updating, and deleting saved host records managed by `esdiag host`.

## Requirements

### Requirement: Explicit Host Lifecycle Subcommands
The system SHALL expose explicit saved-host lifecycle subcommands under `esdiag host`: `add <name>`, `update <name>`, `remove <name>`, `list`, and `auth <name>`. The system SHALL guide users toward those verbs instead of relying on the previous overlapping positional mutation flow.

#### Scenario: Host help shows explicit lifecycle verbs
- **WHEN** the user runs `esdiag host --help`
- **THEN** the help output lists `add`, `update`, `remove`, `list`, and `auth` as available host subcommands

#### Scenario: Legacy positional update syntax is rejected
- **WHEN** the user runs `esdiag host prod-es --secret prod-es-rotated`
- **THEN** the command fails with usage guidance indicating that saved-host mutations must use an explicit host subcommand such as `esdiag host update prod-es`

### Requirement: Explicit Host Creation Command
The system SHALL make `esdiag host add <name>` create-only. `add` MUST require a complete host definition, MUST validate and connection-test the host before saving it, and MUST fail when the host already exists.

The command grammar SHALL use the consistent shape `esdiag host add <name> <target> [--app <app>]`. In explicit host-definition mode, concrete hosts MUST validate and connection-test the provided endpoint before saving. Template-backed hosts MUST validate template structure and supported placeholders before saving, and MUST NOT require a live connection test until a concrete reference is resolved. In materialized-template mode, the system MUST resolve and validate the concrete rendered host before saving it.

When `<target>` is a concrete URL, the system SHALL infer the concrete app when possible and SHALL require `--app` only when the target is ambiguous. When `<target>` is a resolved template reference, the system SHALL derive the concrete app from the resolved or defaulted product and SHALL NOT require `--app` unless the user is creating a template-backed definition whose product cannot be inferred from the target itself. A materialized-template add invocation SHALL support the same flags and defaults as a standard concrete host add invocation, including role overrides and other compatible saved-host options.

For explicit template-backed host-definition mode using `--url-template`, when `--secret` is omitted the system SHALL check for an existing keystore secret whose identifier exactly matches `<name>`. If that same-name secret exists, the system SHALL persist the host using that secret reference as though `--secret <name>` had been supplied. If no matching secret can be confirmed, the system SHALL preserve current behavior and continue without inventing a secret reference. An explicit `--secret <id>` value MUST take precedence over the same-name default.

#### Scenario: Add saves a complete new concrete host
- **GIVEN** no saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host add prod-es http://localhost:9200 --app elasticsearch --secret prod-es-apikey`
- **THEN** the system validates and connection-tests the provided host definition
- **AND** the system saves `prod-es` only after the connection test succeeds

#### Scenario: Add saves a complete new template-backed host
- **GIVEN** no saved host named `elastic-cloud` exists
- **WHEN** the user runs `esdiag host add elastic-cloud https://cloud.elastic.co/api/v1/deployments/{id}/{product}/main-{product}/proxy --url-template --secret elastic-cloud`
- **THEN** the system validates the template host definition
- **AND** the system saves `elastic-cloud` without requiring a live deployment connection test

#### Scenario: Add defaults template host secret to the host name
- **GIVEN** no saved host named `cloud-admin` exists
- **AND** the keystore already contains a secret named `cloud-admin`
- **WHEN** the user runs `esdiag host add cloud-admin https://admin.cloud.com/api/v1/deployments/{id}/{product}/main-{product}/proxy --url-template`
- **THEN** the system validates the template host definition
- **AND** the system saves `cloud-admin` with secret reference `cloud-admin`

#### Scenario: Add preserves current behavior when no matching template secret exists
- **GIVEN** no saved host named `cloud-admin` exists
- **AND** the keystore does not contain a secret named `cloud-admin`
- **WHEN** the user runs `esdiag host add cloud-admin https://admin.cloud.com/api/v1/deployments/{id}/{product}/main-{product}/proxy --url-template`
- **THEN** the system does not invent a secret reference
- **AND** the command follows the same validation and save path it would have used before same-name secret defaulting

#### Scenario: Explicit secret overrides same-name template secret default
- **GIVEN** no saved host named `cloud-admin` exists
- **AND** the keystore already contains a secret named `cloud-admin`
- **WHEN** the user runs `esdiag host add cloud-admin https://admin.cloud.com/api/v1/deployments/{id}/{product}/main-{product}/proxy --url-template --secret platform-admin`
- **THEN** the system uses `platform-admin` as the saved secret reference
- **AND** the same-name default does not override the explicit `--secret` value

#### Scenario: Add saves a rendered concrete host from a template reference
- **GIVEN** a saved template-backed host named `elastic-cloud` exists
- **WHEN** the user runs `esdiag host add netopsco elastic-cloud://415715723947/elasticsearch`
- **THEN** the system resolves the template reference to a concrete host definition
- **AND** validates the rendered host definition
- **AND** saves `netopsco` as a normal concrete saved host

#### Scenario: Add applies standard host flags to a rendered concrete host
- **GIVEN** a saved template-backed host named `elastic-cloud` exists
- **WHEN** the user runs `esdiag host add netopsco elastic-cloud://415715723947/elasticsearch --roles collect,send`
- **THEN** the system resolves the template reference to a concrete host definition
- **AND** applies the same defaulting rules used by standard concrete host creation
- **AND** persists the explicit `collect,send` role override on `netopsco`

#### Scenario: Add rejects duplicate host names
- **GIVEN** a saved host named `prod-es` already exists
- **WHEN** the user runs `esdiag host add prod-es http://localhost:9200 --app elasticsearch`
- **THEN** the command fails with an explicit error indicating that `prod-es` already exists
- **AND** the existing saved host remains unchanged

#### Scenario: Add rejects incomplete host definitions
- **GIVEN** no saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host add prod-es --secret prod-es-apikey`
- **THEN** the command fails with an explicit error indicating that `app` and `url` are required
- **AND** the system does not create or save a partial host record

### Requirement: Incremental Saved Host Updates
The system SHALL treat `esdiag host update <name>` as an update-only operation for a saved host named `<name>` when the invocation provides one or more mutable override flags. The update flow SHALL reuse the saved host's persisted `app` and `url`, apply the provided overrides, validate the merged host record, and save it only after the host connection test succeeds.

#### Scenario: Update a saved secret reference without restating the host definition
- **GIVEN** a saved host named `prod-es` exists with persisted `app`, `url`, and auth metadata
- **WHEN** the user runs `esdiag host update prod-es --secret prod-es-rotated`
- **THEN** the system loads the saved `prod-es` record
- **AND** applies the new secret reference while preserving the saved `app` and `url`
- **AND** validates and connection-tests the merged host record
- **AND** saves the updated `prod-es` record

### Requirement: Partial Override Preservation
The system SHALL preserve saved host fields that are not explicitly overridden by an `esdiag host update <name>` invocation.

#### Scenario: Update roles while preserving auth and transport settings
- **GIVEN** a saved host named `prod-kb` includes a persisted URL, secret reference, and certificate validation setting
- **WHEN** the user runs `esdiag host update prod-kb --roles collect,view`
- **THEN** the saved host keeps its existing URL, auth configuration, and certificate validation setting
- **AND** the persisted role set becomes `collect,view`

### Requirement: Mutable Saved Host Override Support
The system SHALL support in-place `esdiag host update <name>` overrides for saved host authentication fields, role assignments, and certificate validation settings without requiring the user to resupply the full host definition. For certificate validation settings, the system SHALL update the saved value only when `--accept-invalid-certs <bool>` is provided, SHALL preserve the saved value when the flag is omitted, SHALL enable invalid certificate acceptance when the value is `true`, and SHALL remove a previously enabled invalid-certificate override when the value is `false`.

#### Scenario: Replace saved secret-backed auth with an API key override
- **GIVEN** a saved host named `prod-es` currently references secret-backed authentication
- **WHEN** the user runs `esdiag host update prod-es --apikey new-api-key`
- **THEN** the system saves `prod-es` with API key authentication
- **AND** any persisted secret reference for that host is no longer used as the saved auth source

#### Scenario: Enable invalid certificate acceptance on a saved host
- **GIVEN** a saved host named `staging-es` exists with certificate validation disabled
- **WHEN** the user runs `esdiag host update staging-es --accept-invalid-certs true`
- **THEN** the system applies the requested certificate validation setting to the saved host
- **AND** preserves the host's existing `app`, `url`, and auth configuration unless other overrides are supplied

#### Scenario: Omit certificate flag to preserve the saved setting
- **GIVEN** a saved host named `staging-es` exists with `accept_invalid_certs` already enabled
- **WHEN** the user runs `esdiag host update staging-es --roles collect,send`
- **THEN** the system preserves the saved certificate validation setting
- **AND** does not clear or rewrite it only because `--accept-invalid-certs` was omitted

#### Scenario: Remove invalid certificate acceptance from a saved host
- **GIVEN** a saved host named `staging-es` exists with `accept_invalid_certs` enabled
- **WHEN** the user runs `esdiag host update staging-es --accept-invalid-certs false`
- **THEN** the system disables invalid certificate acceptance for the saved host
- **AND** preserves the host's existing `app`, `url`, and auth configuration unless other overrides are supplied

### Requirement: Missing Host Update Guardrail
The system SHALL reject `esdiag host update <name>` invocations for unknown host names rather than inferring a create flow.

#### Scenario: Reject update for unknown host
- **GIVEN** no saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host update prod-es --secret prod-es-rotated`
- **THEN** the command fails with an explicit error indicating that `prod-es` does not exist
- **AND** the system does not create or save a partial host record

### Requirement: Saved Host Listing
The system SHALL provide `esdiag host list` to print a compact table of saved hosts with columns `name`, `app`, and `secret`. The `secret` column SHALL show the saved secret identifier when present and SHALL otherwise be empty. When no hosts are saved, the command SHALL print `No saved hosts`.

#### Scenario: List prints compact host table
- **GIVEN** saved hosts exist in persisted host storage
- **WHEN** the user runs `esdiag host list`
- **THEN** the command prints a compact table with headers `name`, `app`, and `secret`
- **AND** each saved host appears on its own row

#### Scenario: List reports empty host storage
- **GIVEN** no saved hosts exist
- **WHEN** the user runs `esdiag host list`
- **THEN** the command prints `No saved hosts`

### Requirement: Saved Host Authentication Check
The system SHALL provide `esdiag host auth <name>` to test authentication against an existing saved host. `auth` MUST fail when the host does not exist and MUST not modify persisted host storage.

#### Scenario: Auth succeeds for a saved host
- **GIVEN** a saved host named `prod-es` exists with valid authentication
- **WHEN** the user runs `esdiag host auth prod-es`
- **THEN** the system tests the saved host connection using its persisted authentication configuration
- **AND** the command succeeds without changing the saved host record

#### Scenario: Auth rejects missing host
- **GIVEN** no saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host auth prod-es`
- **THEN** the command fails with an explicit error indicating that `prod-es` was not found

### Requirement: Saved Host Deletion From CLI
The system SHALL support deleting an existing saved host record with `esdiag host remove <name>`. The remove command SHALL remove the named host from persisted host storage and SHALL fail with an explicit error when the host does not exist.

#### Scenario: Remove an existing saved host
- **GIVEN** a saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host remove prod-es`
- **THEN** the system removes `prod-es` from persisted host storage
- **AND** does not require `app`, `url`, or connection validation for the remove operation

#### Scenario: Reject remove for an unknown host
- **GIVEN** no saved host named `prod-es` exists
- **WHEN** the user runs `esdiag host remove prod-es`
- **THEN** the command fails with an explicit error indicating that `prod-es` was not found
- **AND** the system leaves persisted host storage unchanged
