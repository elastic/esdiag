## MODIFIED Requirements

### Requirement: Asset Setup Follows Deployment Ownership
For an existing local or remote deployment, initialization SHALL ask whether
the diagnostic cluster needs ESDiag dashboards and agents. On approval, it
SHALL run the same combined pipeline as `esdiag setup output`. It MUST NOT
provision assets unless the user explicitly approves. When the deployment's
Agent Builder license is inactive, initialization SHALL ask separately, with a
default of no, whether to start an Elasticsearch trial license on the named
output. It MUST NOT start a trial without that answer.

When the user approves starting a new binary-owned local core stack,
initialization SHALL treat that approval as approval for the lifecycle's
required ESDiag assets. It SHALL not ask a redundant asset question after the
stack is ready. It SHALL ask no trial question for a new local stack: the
local lifecycle's default activates a trial on a cluster that has never had
one.

#### Scenario: User approves setup for an existing or remote deployment
- **GIVEN** an existing local or remote output deployment validates but required
  ESDiag assets are absent
- **AND** its Agent Builder license is active
- **WHEN** the user approves setup
- **THEN** initialization installs Elasticsearch and Kibana assets against the configured deployment
- **AND** does not ask about a trial license
- **AND** records the running ESDiag asset version

#### Scenario: User approves setup but declines the trial
- **GIVEN** an existing output deployment with an inactive Agent Builder license
- **WHEN** the user approves setup and declines the trial prompt
- **THEN** Elasticsearch assets are installed
- **AND** no `_license/start_trial` request is sent
- **AND** initialization reports that Kibana Agent Builder assets need a license, naming `esdiag setup output --activate-trial`
- **AND** the configured identity, keystore, output deployment, hosts, and job remain valid

#### Scenario: User approves setup and the trial
- **GIVEN** an existing output deployment with an inactive Agent Builder license
- **WHEN** the user approves setup and accepts the trial prompt
- **THEN** a trial license is started before Kibana assets are installed

#### Scenario: User declines setup for an existing or remote deployment
- **GIVEN** an existing local or remote output deployment has required assets absent
- **WHEN** the user declines setup
- **THEN** the configured endpoints remain saved
- **AND** initialization reports that processing or Agent Builder use is not yet ready

#### Scenario: New local stack implies required assets
- **GIVEN** local processing is selected and no usable local stack exists
- **WHEN** the user approves starting a binary-owned core stack
- **THEN** its Rust lifecycle starts a trial license on the new cluster and installs the required ESDiag assets before it reports ready
- **AND** initialization asks neither an asset nor a trial question
- **AND** initialization records the asset version
