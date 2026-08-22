## ADDED Requirements

### Requirement: General Application Configuration
The system SHALL persist local non-secret preferences in a versioned
`~/.esdiag/esdiag.yml` file. The configuration SHALL contain `user`, the
selected initialization `workflow`, an `output` section with `default`,
`authenticated_on`, and `assets_version` when applicable, and a `job` section
with `default` when configured. `output.default` SHALL be the output-host
reference, and `job.default` SHALL be the default saved-job reference. The
configuration MUST NOT contain endpoint credentials, decrypted secrets,
authorization headers, or embedded host and job definitions.

#### Scenario: Configuration contains references only
- **GIVEN** initialization configured an output deployment and default saved job
- **WHEN** `esdiag.yml` is serialized
- **THEN** it contains the selected workflow and, when configured, the output
  host name at `output.default` and default job name at `job.default`
- **AND** successful endpoint authentication and asset setup are recorded, when applicable, at `output.authenticated_on` and `output.assets_version`
- **AND** the referenced host and job bodies remain in `hosts.yml` and `jobs.yml`
- **AND** credential material remains only in `secrets.yml`

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

### Requirement: Canonical Output Deployment Resolution
The system SHALL resolve the processed-diagnostic output as one atomic deployment containing an Elasticsearch send target, its Kibana view target when required, and authentication from the same configuration source. Resolution precedence SHALL be explicit command target, complete `ESDIAG_OUTPUT_*` and `ESDIAG_KIBANA_URL` runtime configuration, the persisted `output.default` host reference, then configuration failure.

#### Scenario: Runtime deployment overrides persisted output
- **GIVEN** `esdiag.yml` references a saved output host
- **AND** a complete `ESDIAG_OUTPUT_URL`, output authentication, and `ESDIAG_KIBANA_URL` are present
- **WHEN** an omitted-output command resolves its deployment
- **THEN** it uses the environment-backed Elasticsearch and Kibana endpoints
- **AND** it applies the output authentication to both products
- **AND** it does not combine either endpoint with the saved deployment

#### Scenario: Persisted output resolves linked viewer
- **GIVEN** `esdiag.yml` references a saved Elasticsearch host with role `send`
- **AND** that host references a saved Kibana host with role `view`
- **WHEN** an operation requiring Elasticsearch and Kibana resolves the output deployment
- **THEN** it uses the saved send host and linked viewer host
- **AND** resolves their referenced secrets through the existing keystore

#### Scenario: Partial runtime deployment fails closed
- **GIVEN** `ESDIAG_OUTPUT_URL` selects an environment-backed deployment
- **AND** an operation requires Kibana but `ESDIAG_KIBANA_URL` is absent
- **WHEN** output deployment resolution runs
- **THEN** it fails with a missing runtime Kibana configuration error
- **AND** it does not borrow the persisted output host's viewer

### Requirement: Shared Output Authentication Contract
Environment-backed output configuration SHALL use `ESDIAG_OUTPUT_APIKEY` or the existing output username/password pair for both Elasticsearch and Kibana. The system MUST NOT require or recognize analysis-specific Elasticsearch URL, Kibana API-key, or Kibana API-key-file variables as a second output deployment configuration.

#### Scenario: API key authenticates both output products
- **GIVEN** `ESDIAG_OUTPUT_URL`, `ESDIAG_KIBANA_URL`, and `ESDIAG_OUTPUT_APIKEY` are configured
- **WHEN** the output deployment constructs its Elasticsearch and Kibana clients
- **THEN** both clients use the configured output API key
- **AND** no duplicate Kibana credential is required
