## MODIFIED Requirements

### Requirement: Fully Configured Startup

`esdiag-local up` SHALL consider a full deployment ready only after
Elasticsearch and Kibana are healthy, ESDiag credentials exist, `esdiag setup env`
has successfully configured both Elasticsearch and Kibana assets, and the
ESDiag web service is healthy. It SHALL consider a core deployment ready only
after Elasticsearch and Kibana are healthy, generated credentials exist, and
the managed host-native ESDiag web service is healthy.

#### Scenario: Successful staged startup

- **GIVEN** runtime validation and image acquisition succeed for full mode
- **WHEN** `esdiag-local up` starts a new deployment
- **THEN** Elasticsearch and Kibana start before credential creation
- **AND** a one-shot ESDiag container completes `esdiag setup env`
- **AND** the ESDiag web container starts only after setup succeeds
- **AND** the command reports success only after all public endpoints pass
  verification

#### Scenario: Successful core startup

- **GIVEN** runtime validation and image acquisition succeed for core mode
- **WHEN** `esdiag-local up` starts a new deployment
- **THEN** Elasticsearch and Kibana start before credential creation
- **AND** the command reports success after their public endpoints pass
  verification
- **AND** no ESDiag setup or web-service container is started
- **AND** the managed host-native ESDiag web service is healthy

#### Scenario: Asset setup fails

- **GIVEN** Elasticsearch and Kibana are healthy in full mode
- **WHEN** the one-shot `esdiag setup env` command fails
- **THEN** `esdiag-local up` exits non-zero and does not report the deployment
  ready
- **AND** relevant logs and generated state are retained for diagnosis and retry

## ADDED Requirements

### Requirement: Setup Command Matches The Pinned ESDiag Image

Both `esdiag-local` and `esdiag local` SHALL derive the full-mode one-shot setup
container command from the state's pinned `ESDIAG_IMAGE` tag every time they
write `compose.yml`. A tag that is a release version lower than `0.18.0` SHALL
produce `command: ["setup"]`. Every other tag, including `0.18.0` and later
releases, their `-SNAPSHOT` builds, and non-version custom tags, SHALL produce
`command: ["setup", "env"]`. The standalone launcher's core mode SHALL run the
matching native binary as `esdiag setup env`. Both launchers SHALL apply the same
rule.

#### Scenario: Existing state pinned to an older image

- **GIVEN** a full-mode state directory whose `.env` pins `docker.elastic.co/esdiag/esdiag:0.17.0`
- **WHEN** a 0.18 launcher runs `up` or `setup` without `upgrade`
- **THEN** the regenerated `compose.yml` runs `command: ["setup"]`
- **AND** asset setup succeeds with the pinned image

#### Scenario: State upgraded to the current image

- **GIVEN** the user runs `esdiag-local upgrade` and the state now pins the 0.18 image
- **WHEN** `compose.yml` is regenerated
- **THEN** the setup container runs `command: ["setup", "env"]`

#### Scenario: Custom image tag

- **GIVEN** the user started full mode with a non-version image tag such as `esdiag:dev`
- **WHEN** `compose.yml` is written
- **THEN** the setup container runs `command: ["setup", "env"]`

#### Scenario: Pinned image tag parity

- **GIVEN** the same pinned image tag
- **WHEN** `esdiag-local` and `esdiag local` each write `compose.yml`
- **THEN** both choose the same setup command

### Requirement: Standalone Trial Activation Defaults To New Clusters

`esdiag-local` SHALL accept `--activate-trial=true|false` on `up`, `setup`, and
`upgrade`. When the option is absent, it SHALL honor
`ESDIAG_SETUP_ACTIVATE_TRIAL`. When both are absent, it SHALL read
`GET _license/trial_status` from the local cluster with the generated `elastic`
credentials before the setup stage. It SHALL resolve to `true` only when
`eligible_to_start_trial` is `true`, and SHALL resolve to `false` when the
request fails. Full mode SHALL pass the resolved value to the one-shot setup
container with `-e ESDIAG_SETUP_ACTIVATE_TRIAL=<value>` on `compose run`. Core
mode SHALL set it in the native `esdiag setup env` environment. The value MUST
NOT be written to the state `.env` or `compose.yml`.

#### Scenario: First full startup

- **GIVEN** a new full-mode deployment whose cluster has never started a trial
- **WHEN** the user executes `esdiag-local up`
- **THEN** the setup container receives `ESDIAG_SETUP_ACTIVATE_TRIAL=true`
- **AND** the deployment is reported ready with Kibana Agent Builder assets installed

#### Scenario: Re-running setup on an existing stack

- **GIVEN** a deployment whose cluster already started a trial
- **WHEN** the user executes `esdiag-local setup` or `esdiag-local upgrade`
- **THEN** setup receives `ESDIAG_SETUP_ACTIVATE_TRIAL=false`

#### Scenario: Opt-out through the environment

- **GIVEN** a new deployment
- **WHEN** the user executes `ESDIAG_SETUP_ACTIVATE_TRIAL=false esdiag-local up`
- **THEN** setup receives `ESDIAG_SETUP_ACTIVATE_TRIAL=false`
- **AND** neither `.env` nor `compose.yml` contains the value

#### Scenario: Control wrapper forwards the option

- **WHEN** the user executes `esdiag-control setup --activate-trial=true`
- **THEN** the delegated launcher receives `--activate-trial=true`
