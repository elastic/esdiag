## ADDED Requirements

### Requirement: Default Local Elasticsearch Output
The generated standalone deployment SHALL start ESDiag in User mode with the local Elasticsearch container configured as its runtime-backed default output. The generated API key SHALL remain runtime-managed in the protected deployment `.env` and MUST NOT require creation or unlocking of the ESDiag keystore before processing.

#### Scenario: Processing uses the local cluster by default
- **GIVEN** `esdiag-local up` has generated a valid Elasticsearch API key
- **WHEN** the ESDiag web container starts without a user-selected saved output host
- **THEN** its active exporter targets the generated local Elasticsearch service using the persisted API key
- **AND** processed documents are not written to stdout

#### Scenario: Default output bypasses keystore bootstrap
- **GIVEN** the local ESDiag keystore does not exist
- **AND** the web container has a complete runtime-provided local Elasticsearch output
- **WHEN** the user starts a processing action
- **THEN** processing does not request a keystore password
- **AND** the runtime-provided API key is not copied into a keystore

### Requirement: Persistent ESDiag User State
The generated standalone Compose deployment SHALL provide a dedicated named volume for ESDiag User-mode artifacts beneath the container's ESDiag configuration directory. The volume SHALL preserve hosts, settings, saved jobs, keystore data, and unlock state across service recreation and routine shutdown, and confirmed reset SHALL remove it with the other deployment volumes.

#### Scenario: ESDiag state survives service recreation
- **GIVEN** the user has created local ESDiag settings, saved jobs, or keystore data
- **WHEN** the ESDiag service is recreated or the deployment is taken down and started again
- **THEN** those artifacts remain available to the replacement container

#### Scenario: Confirmed reset removes ESDiag state
- **GIVEN** the dedicated ESDiag user-state volume exists
- **WHEN** the user executes `esdiag-local reset --force`
- **THEN** the ESDiag user-state volume is removed with the Elasticsearch and Kibana data volumes

### Requirement: Explicit Local Runtime Mode
The generated ESDiag service configuration SHALL explicitly select User mode rather than relying on the binary's implicit default.

#### Scenario: Local service starts in User mode
- **WHEN** `esdiag-local` generates the ESDiag service environment
- **THEN** it declares `ESDIAG_MODE=user`
- **AND** local User-mode web features remain available without identity-aware-proxy headers

### Requirement: Browser-Reachable Kibana Links
The generated standalone deployment SHALL give the setup container the Compose-internal Kibana URL and give the ESDiag web container the host-published Kibana URL used in browser links.

#### Scenario: Setup and browser use different Kibana addresses
- **WHEN** `esdiag-local` generates Compose configuration
- **THEN** setup uses `http://kibana:5601/s/${ESDIAG_KIBANA_SPACE}` to import assets within the Compose network
- **AND** the ESDiag web container uses `http://localhost:${ESDIAG_KIBANA_PORT}/s/${ESDIAG_KIBANA_SPACE}` as its Kibana base URL
- **AND** links returned to the browser do not contain the Compose-only hostname `kibana`

### Requirement: Local Output End-to-End Verification
The standalone local stack test coverage SHALL finish with a live API-key processing job that uses `http://elasticsearch:9200` as both its diagnostic source and its runtime-configured output. The job SHALL use the generated local API key for source authentication, verify that the local node can diagnose itself, and exercise real document indexing so lazily created mapping fields are materialized. Processed documents SHALL NOT be emitted as document output on container stdout.

#### Scenario: Local node diagnoses itself into local output
- **GIVEN** the standalone stack is running with its generated API key
- **WHEN** the final live verification submits a synchronous API-key processing job through the web service with source URL `http://elasticsearch:9200`
- **AND** the job uses the generated API key while the active exporter targets `http://elasticsearch:9200` with that API key
- **THEN** the processing job completes successfully and returns a diagnostic identifier
- **AND** the expected diagnostic documents are queryable from local Elasticsearch
- **AND** fields that are created lazily by real diagnostic indexing are present in the resulting mappings
- **AND** container stdout contains operational logs but not the processed document stream
