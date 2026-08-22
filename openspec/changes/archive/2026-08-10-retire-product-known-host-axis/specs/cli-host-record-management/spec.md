## ADDED Requirements

### Requirement: CLI Host Application Classification
The `esdiag host` commands SHALL accept and display only actual Stack applications as
host applications. Concrete host creation MUST produce a classified application before
connection testing or persistence. A dynamic template definition MAY omit the
application until materialization, and the CLI MUST present that state as unresolved
rather than as `Unknown` or a platform.

#### Scenario: Add concrete host with explicit application
- **GIVEN** the target URL does not identify its application unambiguously
- **WHEN** the user runs `esdiag host add prod-kb https://kibana.example --app kibana`
- **THEN** the CLI constructs a concrete host whose application is `Kibana`
- **AND** validates and connection-tests that classified host before saving it

#### Scenario: Reject platform passed as host application
- **WHEN** the user supplies `--app eck` to a host command
- **THEN** the CLI rejects the value because ECK is a platform rather than an application

#### Scenario: Add unresolved dynamic template
- **GIVEN** a URL template selects its application through the reference product parameter
- **WHEN** the user saves the template without `--app`
- **THEN** the CLI saves an unresolved template with no application placeholder
- **AND** does not require a connection test until the template is materialized

#### Scenario: List unresolved template
- **GIVEN** a saved template-backed host has no application before materialization
- **WHEN** the user runs `esdiag host list`
- **THEN** the application column identifies the host as unresolved
- **AND** does not display `Unknown` or a platform as its application

### Requirement: CLI Template Materialization Classification
When a CLI host target is a template reference, the system SHALL derive the concrete
application from the selected product or the documented default, render the URL, and
validate the resulting resolved host before authentication, connection testing,
collection, or persistence.

#### Scenario: Materialize template with selected application
- **GIVEN** a saved dynamic template named `elastic-cloud`
- **WHEN** the user targets `elastic-cloud://415715723947/kibana`
- **THEN** the CLI materializes a concrete host whose application is `Kibana`
- **AND** validates the rendered route and host roles before use

#### Scenario: Reject unsupported template product
- **GIVEN** a saved dynamic template named `elastic-cloud`
- **WHEN** the user targets `elastic-cloud://415715723947/eck`
- **THEN** the CLI rejects the reference because ECK is not an application target
