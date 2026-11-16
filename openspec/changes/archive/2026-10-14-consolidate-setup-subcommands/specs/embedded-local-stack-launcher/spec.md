## ADDED Requirements

### Requirement: Local Trial Activation Defaults To New Clusters
`esdiag local` SHALL accept `--activate-trial=true|false` on `up`, `setup`,
and `upgrade`, matching `esdiag-local`. When the option is absent, it SHALL
honor `ESDIAG_SETUP_ACTIVATE_TRIAL`. When both are absent, it SHALL resolve
the policy before the setup stage by reading `GET _license/trial_status` from
the local cluster. It SHALL activate a trial only when `eligible_to_start_trial`
is `true`, and SHALL treat a failed request as `false`. The resolved boolean
SHALL be passed to that run's setup stage only, and MUST NOT be written to
local-stack state. Full mode SHALL pass it to the one-shot setup container as
`ESDIAG_SETUP_ACTIVATE_TRIAL`, and core mode SHALL pass it to the in-process
setup pipeline.

#### Scenario: First local startup activates the trial
- **GIVEN** a new local stack whose cluster has never started a trial
- **WHEN** the user runs `esdiag local up`
- **THEN** setup starts a trial license before installing Kibana assets
- **AND** Kibana Agent Builder assets are installed

#### Scenario: Retried first startup still activates the trial
- **GIVEN** a first `esdiag local up` that created credentials but failed before setup completed
- **AND** the cluster has not started a trial
- **WHEN** the user runs `esdiag local up` again
- **THEN** setup starts a trial license

#### Scenario: Later runs leave the license alone
- **GIVEN** a local stack whose cluster already started a trial, active or expired
- **WHEN** the user runs `esdiag local up`, `esdiag local setup`, or `esdiag local upgrade`
- **THEN** no `_license/start_trial` request is sent

#### Scenario: User opts out on first startup
- **GIVEN** a new local stack
- **WHEN** the user runs `esdiag local up --activate-trial=false`
- **THEN** no `_license/start_trial` request is sent
- **AND** startup completes with Elasticsearch assets and a warning that Kibana Agent Builder assets need a license

#### Scenario: Eligibility cannot be read
- **GIVEN** `GET _license/trial_status` fails
- **WHEN** the user runs `esdiag local up` without a trial option
- **THEN** no `_license/start_trial` request is sent
- **AND** the warning names `esdiag local setup --activate-trial=true`

#### Scenario: State does not record the choice
- **WHEN** any local lifecycle command resolves the trial policy
- **THEN** neither `.env` nor `compose.yml` contains `ESDIAG_SETUP_ACTIVATE_TRIAL`

#### Scenario: Launcher parity
- **GIVEN** the same cluster trial status and trial option
- **WHEN** `esdiag local` and `esdiag-local` each run `setup`
- **THEN** both pass the same resolved trial value to setup
