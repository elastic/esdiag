## MODIFIED Requirements

### Requirement: Version-Pinned Official Images

Each released `esdiag-local` artifact SHALL default to exact compatible
versions of the ESDiag, Elasticsearch, and Kibana images hosted on
`docker.elastic.co`. Full mode SHALL use
`docker.elastic.co/esdiag/esdiag:${ESDIAG_VERSION}` for ESDiag containers;
core mode SHALL not require an ESDiag image. Release defaults MUST NOT use
`latest`.

In full mode, ESDiag image selection SHALL use this precedence: an explicit
command-line image option, `ESDIAG_IMAGE_TAG`, the image recorded in existing
deployment state, then the embedded official-image default. An override MUST
apply to both the one-shot setup container and the ESDiag service container.

#### Scenario: First official-image startup

- **GIVEN** the configured release images are not present locally
- **WHEN** the user executes `esdiag-local up --stack=full` without image
  overrides
- **THEN** the script pulls the pinned ESDiag, Elasticsearch, and Kibana images
  from `docker.elastic.co`
- **AND** it does not build a container image

#### Scenario: Core startup does not require an ESDiag image

- **GIVEN** a core deployment has been selected
- **WHEN** the user executes `esdiag-local up`
- **THEN** the script pulls only the required Elasticsearch and Kibana images
- **AND** it does not contact the registry for an ESDiag image

#### Scenario: Explicit custom image startup

- **GIVEN** a caller sets `ESDIAG_IMAGE_TAG=esdiag:custom`, selects full mode,
  and disables pulling
- **WHEN** the caller executes `esdiag-local up`
- **THEN** the script uses `esdiag:custom` for the one-shot setup container and
  ESDiag service container
- **AND** it does not contact the registry for that image

### Requirement: Fully Configured Startup

`esdiag-local up` SHALL consider a full deployment ready only after
Elasticsearch and Kibana are healthy, ESDiag credentials exist, `esdiag setup`
has successfully configured both Elasticsearch and Kibana assets, and the
ESDiag web service is healthy. It SHALL consider a core deployment ready only
after Elasticsearch and Kibana are healthy, generated credentials exist, and
the managed host-native ESDiag web service is healthy.

#### Scenario: Successful staged startup

- **GIVEN** runtime validation and image acquisition succeed for full mode
- **WHEN** `esdiag-local up` starts a new deployment
- **THEN** Elasticsearch and Kibana start before credential creation
- **AND** a one-shot ESDiag container completes `esdiag setup`
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
- **WHEN** the one-shot `esdiag setup` command fails
- **THEN** `esdiag-local up` exits non-zero and does not report the deployment
  ready
- **AND** relevant logs and generated state are retained for diagnosis and retry

### Requirement: Credential and Volume State Coupling

A secure deployment SHALL generate one Elasticsearch API key and persist it in
`.env`. Full mode SHALL use that key for both one-shot `esdiag setup` and the
ESDiag service. Core mode SHALL retain the key only as protected local-stack
state for compatible host-native ESDiag configuration. The script MUST NOT
attempt automatic credential recovery when credential state or initialized
volumes are missing or mismatched.

#### Scenario: Shared API key is used

- **GIVEN** a secure full deployment has generated its ESDiag API key
- **WHEN** setup and service containers are created
- **THEN** both containers receive the same persisted API key

#### Scenario: Core API key remains protected state

- **GIVEN** a secure core deployment has generated its ESDiag API key
- **WHEN** the deployment is inspected or restarted
- **THEN** the key remains in the protected `.env`
- **AND** ordinary lifecycle output does not reveal it

#### Scenario: Environment state is lost

- **GIVEN** initialized deployment volumes exist but the corresponding `.env` is
  missing
- **WHEN** the user executes `esdiag-local up`
- **THEN** the command fails without generating replacement credentials
- **AND** directs the user to restore the state or confirm `reset` for a new
  deployment

#### Scenario: Initialized volume state is lost

- **GIVEN** `.env` records initialized credentials but the corresponding named
  volumes are missing
- **WHEN** the user executes `esdiag-local up`
- **THEN** the command fails without presenting the persisted API key as
  recoverable access
- **AND** directs the user to confirm `reset` before creating a new deployment

### Requirement: Persistent ESDiag User State

Full-mode Compose deployments SHALL provide a dedicated named volume for
ESDiag User-mode artifacts beneath the container's ESDiag configuration
directory. The volume SHALL preserve hosts, settings, saved jobs, keystore
data, and unlock state across service recreation and routine shutdown, and
confirmed reset SHALL remove it with the other deployment volumes. Core mode
MUST NOT create or depend on that volume.

#### Scenario: ESDiag state survives service recreation

- **GIVEN** the user has created local ESDiag settings, saved jobs, or keystore
  data in full mode
- **WHEN** the ESDiag service is recreated or the deployment is taken down and
  started again
- **THEN** those artifacts remain available to the replacement container

#### Scenario: Core mode avoids container user state

- **GIVEN** a core deployment is running
- **WHEN** its Compose configuration and volumes are inspected
- **THEN** no ESDiag user-state volume exists
- **AND** host-native ESDiag state remains outside the container deployment

#### Scenario: Confirmed reset removes ESDiag state

- **GIVEN** the dedicated ESDiag user-state volume exists
- **WHEN** the user executes `esdiag-local reset --force`
- **THEN** the ESDiag user-state volume is removed with the Elasticsearch and
  Kibana data volumes

### Requirement: Full-Mode Container CLI Execution

The standalone launcher SHALL provide
`esdiag-local exec [launcher-options] -- <esdiag-arguments>` as the
container-only ESDiag CLI path. It SHALL require valid managed full-mode state
and running full-stack dependencies. It MUST reject core mode and missing,
incompatible, or stopped full-mode state without changing the deployment.

The command SHALL derive the Compose runtime, project, image, service network,
environment, and `esdiag-data` user-state volume from the managed state. It
SHALL execute the supplied ESDiag arguments in an ephemeral `esdiag` service
container, preserve terminal input/output/error and exit status, and MUST NOT
download or require a separate wrapper artifact.

#### Scenario: Container-only user initializes ESDiag state

- **GIVEN** a healthy full-mode deployment created by `esdiag-local`
- **WHEN** the user executes `esdiag-local exec -- init`
- **THEN** the initializer runs interactively in the full-mode ESDiag user
  state volume
- **AND** it recognizes the managed full stack as its local output deployment
- **AND** it does not offer or attempt to create a nested local stack

#### Scenario: Core mode rejects container CLI execution

- **GIVEN** a managed deployment is in core mode
- **WHEN** the user executes `esdiag-local exec -- agent ask "Summarize"`
- **THEN** the command exits without creating a container
- **AND** explains that core mode requires the native ESDiag binary

### Requirement: Opaque Arguments and Host File Visibility

`exec` SHALL require `--` before ESDiag arguments so launcher options cannot
be confused with child-command options. It SHALL treat all arguments after the
delimiter as opaque ESDiag arguments.

The command SHALL make the caller's working directory available to the
container command for relative paths and SHALL provide an explicit,
repeatable mount option for approved paths outside that directory. It MUST
fail before executing when a referenced host path cannot be made visible to
the container.

#### Scenario: Container CLI processes a working-directory archive

- **GIVEN** a diagnostic archive exists beneath the caller's working directory
- **WHEN** the user executes `esdiag-local exec -- process ./diagnostic.zip`
- **THEN** the command container receives the archive at the corresponding
  path
- **AND** ESDiag processes that archive using the full-mode user state

### Requirement: Internal and Public Kibana Addresses

Container CLI execution SHALL use the Compose-network Kibana address for
service API calls and a separately recorded host-published Kibana viewer URL
for browser handoffs. It SHALL not persist an internal-only Compose hostname as
the user-facing Kibana URL.

#### Scenario: Container Agent Builder returns a browser-reachable link

- **GIVEN** a full-mode deployment is running with Kibana published on a local
  port
- **WHEN** `esdiag-local exec -- agent ask "Summarize"` completes
- **THEN** the Agent Builder request uses the internal Kibana service address
- **AND** the returned Kibana URL uses the host-published local address
