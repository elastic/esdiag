## ADDED Requirements

### Requirement: Interactive First-Run Workflow
The CLI SHALL provide `esdiag init` as an interactive staged workflow. It SHALL
first determine whether the user will process diagnostics or only collect them.
Processing workflows SHALL then determine whether the user will collect new
diagnostics, process existing diagnostics, or do both. The workflow SHALL
configure only the identity, keystore, output deployment, collect host, and
default job stages required by that selection.

The workflow SHALL use in-process domain APIs and MUST NOT invoke an unrelated
ESDiag executable, the standalone `esdiag-local` script, or arbitrary external
helper executable. It MAY invoke the binary-owned Rust local-stack lifecycle
and its managed native web-service child only to start a user-approved local
core deployment for a local processing workflow.

#### Scenario: User initializes collection-only workflow
- **GIVEN** no ESDiag local state exists
- **WHEN** the user selects only collecting diagnostics and completes `esdiag init`
- **THEN** a collect host, default collection job, and `esdiag.yml` are persisted
- **AND** no output deployment or output asset setup is requested

#### Scenario: User initializes existing-diagnostic processing
- **GIVEN** no ESDiag local state exists
- **WHEN** the user selects processing existing diagnostics and completes
  `esdiag init`
- **THEN** a keystore, linked output hosts, and `esdiag.yml` are persisted
- **AND** no collect host or default saved job is requested

#### Scenario: User initializes collection and processing
- **GIVEN** no ESDiag local state exists
- **WHEN** the user selects collecting and processing diagnostics
- **THEN** a keystore, linked output hosts, a collect host, a default
  collect-and-process job, and `esdiag.yml` are persisted
- **AND** the final result identifies the configured references without exposing credentials

### Requirement: Initialization Is Resumable And Non-Destructive
Initialization SHALL inspect existing state before each stage, reuse valid completed state by default, and require explicit confirmation before replacing a configured user preference, host, secret, or saved job. It SHALL persist only validated domain values and SHALL write each applicable `esdiag.yml` field after its corresponding stage validates. Initialization readiness SHALL be derived from validated configuration references and referenced domain state, not from configuration-file existence alone.

#### Scenario: Interrupted initialization resumes
- **GIVEN** a prior initialization created the keystore and output hosts but stopped before creating a job
- **WHEN** the user runs `esdiag init` again
- **THEN** the workflow recognizes and offers the valid existing stages
- **AND** resumes at the first incomplete or explicitly changed stage

#### Scenario: Existing job is protected
- **GIVEN** the requested default job name already exists
- **WHEN** initialization would replace it
- **THEN** the user must explicitly confirm replacement
- **AND** declining leaves the saved job unchanged

### Requirement: Secret Input Remains Outside Agent Conversations
Initialization SHALL obtain required keystore passwords, API keys, and host passwords through hidden controlling-terminal prompts or existing secure credential sources. It MUST NOT echo those values, include them in command outcomes or logs, or accept a workflow that requires an agent to receive the values.

#### Scenario: Interactive API key entry
- **WHEN** initialization prompts for an output API key
- **THEN** entered characters are hidden
- **AND** the value is stored through the encrypted keystore API
- **AND** neither stdout nor stderr contains the key

#### Scenario: Required secret without terminal
- **GIVEN** no secure existing credential source supplies a required secret
- **AND** no controlling terminal is available
- **WHEN** `esdiag init` reaches that secret stage
- **THEN** it exits without reading the secret from ordinary stdin
- **AND** reports how to resume from an interactive terminal

### Requirement: Output Deployment Is Linked And Validated
The output stage SHALL create or select an Elasticsearch send host and a Kibana view host, link the send host to the viewer, and validate both endpoints using existing clients before making the deployment active. It SHALL reuse one secret reference by default when both products share authentication and SHALL permit explicitly selected distinct existing secret references.

#### Scenario: Shared output credential is configured once
- **GIVEN** Elasticsearch and Kibana accept the same API key
- **WHEN** the user configures the output deployment
- **THEN** the wizard stores the credential once in the keystore
- **AND** both saved hosts reference that secret
- **AND** the send host's viewer references the Kibana host

#### Scenario: Output validation fails
- **WHEN** either output endpoint rejects its resolved credentials
- **THEN** initialization does not set the output as active
- **AND** preserves any previously active valid output reference

### Requirement: Asset Setup Follows Deployment Ownership
For an existing local or remote deployment, initialization SHALL ask whether
the diagnostic cluster needs ESDiag dashboards and agents. On approval, it
SHALL run the existing version-compatible setup operation. It MUST NOT
provision assets unless the user explicitly approves.

When the user approves starting a new binary-owned local core stack,
initialization SHALL treat that approval as approval for the lifecycle's
required ESDiag assets. It SHALL not ask a redundant asset question after the
stack is ready.

#### Scenario: User approves setup for an existing or remote deployment
- **GIVEN** an existing local or remote output deployment validates but required
  ESDiag assets are absent
- **WHEN** the user approves setup
- **THEN** initialization runs the existing setup behavior against the configured deployment
- **AND** records the running ESDiag asset version

#### Scenario: User declines setup for an existing or remote deployment
- **GIVEN** an existing local or remote output deployment has required assets absent
- **WHEN** the user declines setup
- **THEN** the configured endpoints remain saved
- **AND** initialization reports that processing or Agent Builder use is not yet ready

#### Scenario: New local stack implies required assets
- **GIVEN** local processing is selected and no usable local stack exists
- **WHEN** the user approves starting a binary-owned core stack
- **THEN** its Rust lifecycle installs the required ESDiag assets before it
  reports ready
- **AND** initialization records the asset version without a second asset prompt

### Requirement: Required Saved Job Matches Workflow
Initialization SHALL create a valid default saved job only for workflows that
collect diagnostics. A collect-and-process job SHALL use the configured collect
host and output send host so a run indexes a diagnostic. A collection-only job
SHALL remain supported. Processing existing diagnostics SHALL not require a
saved job.

#### Scenario: Default processing job is created
- **WHEN** the user accepts the default job shape
- **THEN** the saved job collects from the selected collect host
- **AND** processes to the configured output host
- **AND** its name becomes `job.default` in `esdiag.yml`

#### Scenario: Collect-only job is explicit
- **WHEN** the user selects a collection-only workflow
- **THEN** initialization identifies that the job produces an archive rather than indexed diagnostic data
- **AND** persists it as the default job

#### Scenario: Existing diagnostic processing has no default job
- **WHEN** the user selects processing existing diagnostics
- **THEN** initialization completes after the output deployment is configured
- **AND** no default job reference is persisted

