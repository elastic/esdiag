## MODIFIED Requirements

### Requirement: General Application Configuration
The system SHALL persist local non-secret preferences in a versioned
`~/.esdiag/esdiag.yml` file. The configuration SHALL contain `user`, the
selected initialization `workflow`, an `output` section with `default`,
`authenticated_on`, and `assets_version` when applicable, a `job` section
with `default` when configured, and an optional `logs` section with `host`,
`path`, and `retention_days` when configured. `output.default` SHALL be the
output-host reference, `job.default` SHALL be the default saved-job reference,
and `logs.host` SHALL be a saved Elasticsearch host reference with role `send`.
The configuration MUST NOT contain endpoint credentials, decrypted secrets,
authorization headers, or embedded host and job definitions.

#### Scenario: Configuration contains references only
- **GIVEN** initialization configured an output deployment, default saved job, and logs target
- **WHEN** `esdiag.yml` is serialized
- **THEN** it contains the selected workflow and, when configured, the output
  host name at `output.default`, default job name at `job.default`, and logs
  settings at `logs.host`, `logs.path`, and `logs.retention_days`
- **AND** the referenced host and job bodies remain in `hosts.yml` and `jobs.yml`
- **AND** credential material remains only in `secrets.yml`

#### Scenario: Existing configuration without logs section remains readable
- **GIVEN** an existing version-one `esdiag.yml` has no `logs` field
- **WHEN** the running application loads the configuration
- **THEN** it loads with `logs.host` as `None`, `logs.path` as `None`, and `logs.retention_days` as `None`
- **AND** no migration or rewrite is required

#### Scenario: Logs host reference validation checks role and application
- **GIVEN** `esdiag.yml` sets `logs.host` to a host name
- **WHEN** configuration reference validation runs
- **THEN** validation succeeds only if the named host exists in `hosts.yml`, resolves to `Application::Elasticsearch`, and includes role `send`
- **AND** unlike `output.default`, `logs.host` does NOT require a linked Kibana viewer

#### Scenario: Existing configuration has no workflow
- **GIVEN** an existing version-one `esdiag.yml` has no workflow field
- **WHEN** the running application loads the configuration
- **THEN** it remains readable
- **AND** initialization requests workflow selection before claiming readiness

#### Scenario: Unknown configuration version is rejected
- **GIVEN** `esdiag.yml` declares a schema version newer than the running binary supports
- **WHEN** the application loads local configuration
- **THEN** loading fails with an actionable compatibility error
- **AND** no configuration file is rewritten
