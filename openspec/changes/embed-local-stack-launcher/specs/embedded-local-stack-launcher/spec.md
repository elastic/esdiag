## Purpose

Provide binary-first users a version-matched local-stack launcher without a
separate script installation while preserving explicit containerized operation.

## ADDED Requirements

### Requirement: Binary-Owned Local Stack Command

The ESDiag binary SHALL provide `esdiag local <command> [options]` for local
stack lifecycle operations. It SHALL execute the lifecycle through Rust code
owned by the running binary without requiring a persistent `esdiag-local` file,
repository checkout, release download, or Bash interpreter.

#### Scenario: Binary-first user starts a stack

- **GIVEN** a user has installed only the ESDiag binary and a supported
  container runtime
- **WHEN** the user executes `esdiag local up`
- **THEN** the local stack lifecycle starts without downloading or installing a
  standalone script
- **AND** the command reports structured lifecycle completion and progress
  without executing the standalone script

### Requirement: Standalone Native Compatibility

A standalone launcher selecting host-native mode SHALL do so only when the
discovered `esdiag` executable reports that exact version; an absent, unreadable,
failing, or mismatched executable MUST NOT select native mode. `esdiag local`
SHALL use its own running binary for core mode.

#### Scenario: Matching binary selects native mode

- **GIVEN** a standalone launcher and a host `esdiag` executable report the
  same ESDiag version
- **WHEN** automatic stack selection starts a new deployment
- **THEN** the deployment selects core mode
- **AND** it does not pull or start an ESDiag container

#### Scenario: Mismatched binary does not select native mode

- **GIVEN** a host `esdiag` executable reports a different version than the
  launcher
- **WHEN** automatic stack selection starts a new deployment
- **THEN** the deployment selects full mode
- **AND** it reports why host-native mode was not selected

### Requirement: Explicit Local Stack Modes

`esdiag local up` and the standalone launcher SHALL accept
`--stack=auto|full|core`. Native `auto` SHALL resolve a new deployment to core.
Standalone `auto` SHALL resolve to core when a compatible native binary is
available and to full mode otherwise.
`full` SHALL force the ESDiag setup and web-service containers. `core` SHALL
force a deployment containing only Elasticsearch and Kibana. The resolved mode
for a managed state directory SHALL remain stable on later automatic starts
until the user explicitly selects another mode.

#### Scenario: User forces full mode

- **GIVEN** a compatible host-native ESDiag binary is available
- **WHEN** the user executes `esdiag local up --stack=full`
- **THEN** the deployment starts the ESDiag setup and web-service containers
- **AND** it does not select core mode

#### Scenario: Existing mode is stable

- **GIVEN** automatic startup previously resolved a state directory to full
  mode
- **AND** a matching native ESDiag binary later becomes available
- **WHEN** the user executes `esdiag local up --stack=auto`
- **THEN** the deployment remains in full mode
- **AND** the user must explicitly select `--stack=core` to change it

### Requirement: Core Stack Lifecycle

Core mode SHALL provision and verify Elasticsearch and Kibana containers, then
start the running version-matched ESDiag binary as a managed native User-mode
web service. It MUST NOT pull, start, or restart the ESDiag setup or
web-service containers, or create an ESDiag container user-state volume. The
native service SHALL use the generated local output deployment, bind the
configured ESDiag port, and provide the same browser-reachable web UI endpoint
as full mode.

Core-mode lifecycle commands SHALL identify and safely manage the native web
service. `status` and `logs` SHALL include its state and logs; restarting
ESDiag SHALL restart the managed native service; and `down` and confirmed
`reset` SHALL stop it without stopping an unrelated process when stale managed
state is detected.

#### Scenario: Core mode omits ESDiag containers

- **WHEN** the user starts a core deployment
- **THEN** Elasticsearch and Kibana become healthy
- **AND** no ESDiag image is pulled
- **AND** the host-native ESDiag web service becomes available at the configured
  ESDiag endpoint

#### Scenario: Native web service is restarted in core mode

- **GIVEN** a managed deployment is in core mode
- **WHEN** the user requests an ESDiag restart
- **THEN** the command restarts the managed native ESDiag web service
- **AND** it does not create an ESDiag container

### Requirement: Shared Stack State Contract

`esdiag-local` and `esdiag local` SHALL use the same managed stack-state
directory, schema version, environment keys, generated credentials, configured
ports, resolved stack mode, and lifecycle-log locations. They SHALL interpret
valid shared stack state consistently and preserve valid generated credentials
on repeated lifecycle commands. A caller that cannot operate the recorded mode
MUST fail without changing state and explain which entry point or explicit mode
selection is required.

#### Scenario: Binary manages a standalone full deployment

- **GIVEN** `esdiag-local` created a valid full deployment
- **WHEN** the matching `esdiag local status` command inspects that state
- **THEN** it reports the same configured endpoints and resolved full mode
- **AND** it does not rotate credentials or change the deployment mode

#### Scenario: Standalone launcher cannot operate core mode

- **GIVEN** native ESDiag created a valid core deployment
- **AND** no compatible native ESDiag binary is available to the standalone
  launcher
- **WHEN** the user invokes `esdiag-local up` against that state
- **THEN** the command exits without changing the deployment
- **AND** explains how to use the compatible binary or explicitly select full
  mode

### Requirement: Runtime User State Is Mode-Owned

Full mode SHALL own ESDiag user configuration, saved jobs, keystore, and unlock
state in its container runtime state. Core mode SHALL own those artifacts in
native ESDiag state. Phase one MUST NOT copy, bind mount, merge, or otherwise
migrate those runtime-user artifacts when a user explicitly changes stack mode.
Mode-change guidance SHALL state that each runtime retains its own user state.

#### Scenario: Explicit mode change does not migrate configuration

- **GIVEN** a full deployment has container-owned ESDiag user state
- **WHEN** the user explicitly changes the deployment to core mode
- **THEN** the full-mode user state remains unchanged
- **AND** core mode does not import its hosts, jobs, keystore, or unlock state
- **AND** the command explains that migration is not available

### Requirement: Equivalent Secure Local Web Experience

Both modes SHALL expose ESDiag at the configured loopback ESDiag port, use the
same generated local output deployment, and provide browser-launch and
Kibana-password clipboard behavior with the same user controls. Core mode
MUST pass generated output credentials to the managed native web service
without exposing them in process arguments, ordinary output, or durable native
user configuration unless the user explicitly configures a saved output.

#### Scenario: Core mode opens the local web UI securely

- **GIVEN** a core deployment starts successfully with browser launch enabled
- **WHEN** the managed native web service becomes healthy
- **THEN** it is reachable only through the configured loopback ESDiag endpoint
- **AND** browser and clipboard behavior follows the same opt-in and opt-out
  controls as full mode
- **AND** ordinary lifecycle output does not reveal generated credentials

### Requirement: Equivalent Setup and Lifecycle Semantics

Full and core modes SHALL apply the same ESDiag setup assets, Agent Builder
license handling, readiness criteria, and lifecycle operation semantics against
the generated Elasticsearch and Kibana deployment. Full mode SHALL execute
setup through its containerized ESDiag runtime; core mode SHALL execute the
same setup capability through the matching native binary. Differences in
container-internal versus host-published endpoint addresses MUST NOT change the
resulting configured assets.

#### Scenario: Core setup matches full setup

- **GIVEN** identical empty local Elasticsearch and Kibana deployments
- **WHEN** full mode and core mode complete their respective setup paths
- **THEN** both deployments contain the same required ESDiag assets and Agent
  Builder configuration
- **AND** both report ready only after their applicable web service is healthy

### Requirement: Native Local Command Output Contract

`esdiag local` SHALL follow the ESDiag CLI's finite structured-outcome contract
while writing lifecycle progress to standard error. The standalone
`esdiag-local` script SHALL retain its human-oriented output contract. The two
entry points SHALL nevertheless report equivalent lifecycle state, endpoints,
mode, and safe failure guidance without exposing credentials.

#### Scenario: Successful native lifecycle command

- **WHEN** a user runs `esdiag local status`
- **THEN** standard output contains the selected structured outcome format
- **AND** operational lifecycle progress is isolated to standard error
- **AND** the outcome identifies the same mode and endpoints the standalone
  launcher reports for the shared deployment

### Requirement: Native Initialization Can Provision a Core Stack

When an interactive native `esdiag init` processing workflow selects a local output
deployment and no usable local stack exists, initialization SHALL offer to start
the binary-owned Rust lifecycle in core mode. Accepting SHALL return to the same
interactive workflow after Elasticsearch and Kibana are ready, using the
generated local deployment state. Declining SHALL leave no new local stack
running and SHALL let the user choose a different output deployment.

Native initialization MUST NOT download a launcher, invoke an unrelated ESDiag
executable, execute the standalone script, or invoke an arbitrary external
helper.

#### Scenario: Initialization starts a requested local core stack

- **GIVEN** a user selected local processing during initialization
- **AND** no usable local stack exists
- **WHEN** the user accepts the offer to start one
- **THEN** initialization starts a core deployment through the Rust lifecycle
- **AND** resumes local output configuration without asking for a standalone
  launcher installation

#### Scenario: User declines local stack startup

- **GIVEN** initialization offered to start a local core deployment
- **WHEN** the user declines
- **THEN** initialization does not create containers or deployment state
- **AND** offers the remote output path

### Requirement: Native Lifecycle Updates Follow Binary Updates

`esdiag local update` SHALL not self-replace binary-owned lifecycle code. It
MUST direct users to update the ESDiag binary through its installation channel.
The downloaded standalone launcher SHALL retain its explicit self-update
behavior.

#### Scenario: User requests a native lifecycle update

- **WHEN** a user executes `esdiag local update`
- **THEN** the command does not download or replace a launcher file
- **AND** it explains that updating the ESDiag binary updates its local
  lifecycle
