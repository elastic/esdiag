# standalone-local-stack Specification

## Purpose
TBD - created by archiving change add-standalone-esdiag-local. Update Purpose after archive.
## Requirements
### Requirement: Standalone Local Stack Artifact
The project SHALL distribute `esdiag-local` as a single executable shell artifact that manages a local ESDiag stack without reading any repository-owned file. The script MAY generate durable runtime configuration after execution, but MUST NOT require `Cargo.toml`, Git metadata, `example.env`, `docker/Dockerfile`, repository Compose files, or a repository-relative working directory.

#### Scenario: Starting outside a repository
- **GIVEN** a user has downloaded only the released `esdiag-local` script to a new machine with a supported container runtime, Compose provider, and required host utilities
- **WHEN** the user executes `esdiag-local up` from a directory that is not an ESDiag source checkout
- **THEN** the script generates all required runtime configuration in its managed state directory
- **AND** no repository file is accessed

### Requirement: Managed Runtime State
`esdiag-local` SHALL store generated environment configuration, Compose configuration, and failure logs beneath `${ESDIAG_LOCAL_DIR:-$HOME/.esdiag/local}` unless an explicit state-directory option is supplied. Credential-bearing files MUST be created with restrictive permissions, and command behavior MUST be independent of the script location and current working directory.

#### Scenario: Reusing generated state
- **GIVEN** `esdiag-local up` has previously generated configuration and credentials
- **WHEN** the user invokes another lifecycle command from a different working directory
- **THEN** the command resolves and reuses the same managed deployment state
- **AND** existing valid credentials are not rotated

### Requirement: Repository State Isolation
Delegated `esdiag-control` lifecycle commands SHALL default to state beneath `${repository_root}/target/esdiag-local`, separate from the standalone `${ESDIAG_LOCAL_DIR:-$HOME/.esdiag/local}` deployment. An explicit caller-supplied state directory MUST take precedence over both defaults.

#### Scenario: Repository build does not alter standalone state
- **GIVEN** a standalone official-image deployment exists in the user's default state directory
- **WHEN** the user executes `esdiag-control up` from a source checkout without a state override
- **THEN** the delegated deployment uses `${repository_root}/target/esdiag-local`
- **AND** does not change the standalone deployment's image selection, credentials, Compose configuration, containers, or volumes

### Requirement: Bash Platform Compatibility
`esdiag-local` SHALL target Bash 3.2 or newer on Linux and macOS. Platform-dependent behavior for path resolution, checksums, filesystem metadata, resource inspection, browser launch, and clipboard access MUST use context-aware adapters rather than GNU-only assumptions.

#### Scenario: Running with macOS system Bash
- **GIVEN** a supported macOS host provides Bash 3.2 and BSD userland tools
- **WHEN** the user executes an `esdiag-local` lifecycle command
- **THEN** the script selects macOS-compatible adapters
- **AND** does not require GNU-specific command options or Bash features newer than 3.2

#### Scenario: Running on Linux
- **GIVEN** a supported Linux host provides Bash and standard Linux userland tools
- **WHEN** the user executes an `esdiag-local` lifecycle command
- **THEN** the script selects Linux-compatible adapters

### Requirement: Safe Environment Configuration
The generated `.env` SHALL be treated as configuration data and MUST NOT be executed with `source`, `.`, `eval`, or equivalent shell evaluation. `esdiag-local` SHALL parse only documented allowlisted keys and SHALL update the file through a restrictive temporary file followed by atomic rename.

#### Scenario: Environment file contains shell syntax
- **GIVEN** the managed `.env` contains a value or line that is not valid allowlisted configuration
- **WHEN** `esdiag-local` reads the file
- **THEN** the content is not executed as shell code
- **AND** the command rejects or safely ignores it according to documented validation rules

### Requirement: Environment-Configured Host Ports
The host ports for Elasticsearch, Kibana, and ESDiag SHALL be configured in `.env` through `ESDIAG_ELASTICSEARCH_PORT`, `ESDIAG_KIBANA_PORT`, and `ESDIAG_PORT`, defaulting to `9200`, `5601`, and `2501`. Port command-line options SHALL NOT be required.

#### Scenario: Custom ports are configured
- **GIVEN** the three documented port variables contain distinct available ports in the range 1 through 65535
- **WHEN** the user executes `esdiag-local up`
- **THEN** generated host bindings and reported endpoints use those ports

#### Scenario: Port configuration is invalid
- **GIVEN** a configured port is non-numeric, out of range, duplicated, or already unavailable
- **WHEN** the user executes `esdiag-local up`
- **THEN** validation fails before containers are created
- **AND** identifies the invalid or conflicting port

### Requirement: Container Runtime Detection
`esdiag-local` SHALL detect and validate a supported container runtime using the existing Podman-first, Docker-fallback pattern, while allowing an explicit runtime override. It MUST verify engine reachability and compatible Compose capabilities before modifying deployment state.

#### Scenario: Podman is available
- **GIVEN** Podman and its compatible Compose provider are installed and reachable
- **WHEN** no runtime override is supplied
- **THEN** `esdiag-local` selects Podman and its supported Compose command forms

#### Scenario: Podman is absent and Docker is available
- **GIVEN** Podman is unavailable and Docker with Compose is installed and reachable
- **WHEN** no runtime override is supplied
- **THEN** `esdiag-local` selects Docker

#### Scenario: Selected provider lacks a required capability
- **GIVEN** the detected or explicitly selected Compose provider cannot perform required health or no-pull behavior
- **WHEN** startup validation runs
- **THEN** the command exits before modifying deployment state with actionable compatibility guidance

### Requirement: Version-Pinned Official Images
Each released `esdiag-local` artifact SHALL default to exact compatible versions of the ESDiag, Elasticsearch, and Kibana images hosted on `docker.elastic.co`. The default ESDiag image SHALL be `docker.elastic.co/esdiag/esdiag:${ESDIAG_VERSION}`, and release defaults MUST NOT use `latest`.

ESDiag image selection SHALL use this precedence: an explicit command-line image option, `ESDIAG_IMAGE_TAG`, the image recorded in existing deployment state, then the embedded official-image default. An override MUST apply to both the one-shot setup container and the ESDiag service container.

#### Scenario: First official-image startup
- **GIVEN** the configured release images are not present locally
- **WHEN** the user executes `esdiag-local up` without image overrides
- **THEN** the script pulls the pinned ESDiag, Elasticsearch, and Kibana images from `docker.elastic.co`
- **AND** it does not build a container image

#### Scenario: Explicit custom image startup
- **GIVEN** a caller sets `ESDIAG_IMAGE_TAG=esdiag:custom` and disables pulling
- **WHEN** the caller executes `esdiag-local up`
- **THEN** the script uses `esdiag:custom` for the one-shot setup container and ESDiag service container
- **AND** does not contact the registry for that image

### Requirement: Fully Configured Startup
`esdiag-local up` SHALL consider the deployment ready only after Elasticsearch and Kibana are healthy, ESDiag credentials exist, `esdiag setup` has successfully configured both Elasticsearch and Kibana assets, and the ESDiag web service is healthy.

#### Scenario: Successful staged startup
- **GIVEN** runtime validation and image acquisition succeed
- **WHEN** `esdiag-local up` starts a new deployment
- **THEN** Elasticsearch and Kibana start before credential creation
- **AND** a one-shot ESDiag container completes `esdiag setup`
- **AND** the ESDiag web container starts only after setup succeeds
- **AND** the command reports success only after all public endpoints pass verification

#### Scenario: Asset setup fails
- **GIVEN** Elasticsearch and Kibana are healthy
- **WHEN** the one-shot `esdiag setup` command fails
- **THEN** `esdiag-local up` exits non-zero and does not report the deployment ready
- **AND** relevant logs and generated state are retained for diagnosis and retry

### Requirement: Idempotent Lifecycle Commands
`esdiag-local` SHALL provide `up`, `down`, `status`, `logs`, `setup`, `auth`, `secrets`, `reset`, `update`, `help`, and `version`. Repeated `up` invocations MUST reconcile the existing deployment without deleting data or rotating valid credentials, while `down` MUST preserve generated state and volumes.

#### Scenario: Repeating startup
- **GIVEN** the local stack is already configured and running
- **WHEN** the user executes `esdiag-local up` again
- **THEN** the command completes successfully using the existing credentials and persistent data
- **AND** duplicate containers, networks, and volumes are not created

#### Scenario: Routine shutdown
- **GIVEN** the local stack has persistent data and generated credentials
- **WHEN** the user executes `esdiag-local down`
- **THEN** the deployment containers are removed or stopped according to the documented Compose lifecycle
- **AND** its volumes, credentials, and generated configuration remain available for the next `up`

#### Scenario: Destructive reset
- **GIVEN** the local stack has persistent data
- **WHEN** the user executes `esdiag-local reset` without interactive confirmation or `--force`
- **THEN** the script does not remove the persistent data

### Requirement: Explicit Upgrade Control
`esdiag-local` MUST retain the versions recorded in existing deployment state and MUST NOT silently change image versions when invoked from a newer script. Changing those versions SHALL require `up --upgrade` or explicit version overrides. A successful script self-update SHALL report that stack versions remain pinned and direct the user to `esdiag-local up --upgrade`.

#### Scenario: New script finds older state
- **GIVEN** an existing deployment is pinned to an older compatible image set
- **AND** the user replaces the script with a newer release
- **WHEN** the user executes `esdiag-local up` without `--upgrade`
- **THEN** the existing image versions remain selected
- **AND** the command explains how to request an upgrade

#### Scenario: Explicit stack upgrade succeeds
- **GIVEN** a newer script contains new compatible image defaults and an older deployment exists
- **WHEN** the user executes `esdiag-local up --upgrade`
- **THEN** the script stages the new configuration, pulls and validates the new images, reruns asset setup, and verifies the deployment
- **AND** commits the new versions to durable state only after the deployment reaches ready

#### Scenario: Explicit stack upgrade fails
- **GIVEN** an existing deployment has valid prior version state
- **WHEN** `esdiag-local up --upgrade` fails before the upgraded deployment reaches ready
- **THEN** the new version state is not committed
- **AND** the prior version state remains available for recovery with a normal `up`

### Requirement: Secure Local Defaults
The generated deployment SHALL enable Elastic security, bind host-facing service ports to loopback, protect credential files, and use separate persistent volumes for Elasticsearch and Kibana. Security SHALL NOT be disabled through a command-line option.

#### Scenario: Default secure deployment
- **GIVEN** the user has not requested host binding overrides
- **WHEN** `esdiag-local up` generates the deployment
- **THEN** Elasticsearch security is enabled
- **AND** Elasticsearch, Kibana, and ESDiag ports bind to `127.0.0.1`
- **AND** Elasticsearch and Kibana data use separate named volumes

### Requirement: Credential and Volume State Coupling
A secure deployment SHALL generate one Elasticsearch API key, persist it in `.env`, and use that same key for both one-shot `esdiag setup` and the ESDiag service. The script MUST NOT attempt automatic credential recovery when credential state or initialized volumes are missing or mismatched.

#### Scenario: Shared API key is used
- **GIVEN** a secure deployment has generated its ESDiag API key
- **WHEN** setup and service containers are created
- **THEN** both containers receive the same persisted API key

#### Scenario: Environment state is lost
- **GIVEN** initialized deployment volumes exist but the corresponding `.env` is missing
- **WHEN** the user executes `esdiag-local up`
- **THEN** the command fails without generating replacement credentials
- **AND** directs the user to restore the state or confirm `reset` for a new deployment

#### Scenario: Initialized volume state is lost
- **GIVEN** `.env` records initialized credentials but the corresponding named volumes are missing
- **WHEN** the user executes `esdiag-local up`
- **THEN** the command fails without presenting the persisted API key as recoverable access
- **AND** directs the user to confirm `reset` before creating a new deployment

### Requirement: Explicit Secret Retrieval
`esdiag-local` SHALL provide `secrets password` for the generated `elastic` user password and `secrets apikey` for the generated Elasticsearch API key. On success, each command MUST write only the requested raw value to standard output, with diagnostics isolated to standard error.

#### Scenario: Password in command substitution
- **GIVEN** a secure local stack has generated an Elastic password
- **WHEN** the user evaluates `password=$(esdiag-local secrets password)`
- **THEN** `password` contains only the generated `elastic` user password
- **AND** no label, color code, timestamp, or log message is included

#### Scenario: API key in a pipeline
- **GIVEN** a secure local stack has generated its ESDiag Elasticsearch API key
- **WHEN** the user executes `esdiag-local secrets apikey` in a pipeline
- **THEN** standard output contains only the API key
- **AND** the command exits successfully

#### Scenario: Requested secret is unavailable
- **GIVEN** security is disabled or the requested credential has not been generated
- **WHEN** the user executes an `esdiag-local secrets` subcommand
- **THEN** the command exits non-zero without writing a value to standard output
- **AND** writes a diagnostic message to standard error

### Requirement: Secrets Excluded from Operational Output
`esdiag-local status`, `auth`, `logs`, help, and debug logging MUST NOT reveal the Elastic password or Elasticsearch API key.

#### Scenario: Inspecting deployment status
- **GIVEN** a secure local stack has generated credentials
- **WHEN** the user executes `esdiag-local status`
- **THEN** the command reports deployment health and endpoints
- **AND** does not include the password or API key

### Requirement: Platform-Aware Clipboard Assistance
`esdiag-local` help SHALL provide clipboard commands appropriate to detected host utilities. On macOS the documented form SHALL be `esdiag-local secrets password | pbcopy`; supported Linux and WSL environments SHALL use detected `wl-copy`, `xclip`, `xsel`, or `clip.exe` equivalents.

#### Scenario: macOS clipboard help
- **GIVEN** `esdiag-local` is running on macOS with `pbcopy` available
- **WHEN** the user requests secrets or startup help
- **THEN** the output includes `esdiag-local secrets password | pbcopy`

### Requirement: Password Copy Before Browser Launch
When secure `esdiag-local up` is configured to open a browser, the command SHALL make a best-effort attempt to copy the Elastic password with the detected clipboard utility immediately before launching the browser. It MUST NOT print the password, MUST NOT copy the API key, and MUST allow automatic password copying to be disabled with `--copy-password=false`.

#### Scenario: Supported clipboard and browser launch
- **GIVEN** secure startup has completed and a supported clipboard utility is available
- **AND** browser launch and password copying are enabled
- **WHEN** `esdiag-local up` reaches the browser-launch stage
- **THEN** it copies the Elastic password to the clipboard immediately before opening Kibana
- **AND** reports that the password was copied without printing its value

#### Scenario: Clipboard copying is disabled
- **GIVEN** the user supplied `--copy-password=false`
- **WHEN** secure startup launches the browser
- **THEN** the password is not sent to any clipboard utility

#### Scenario: Clipboard utility fails
- **GIVEN** secure startup is ready to launch the browser but clipboard copying fails
- **WHEN** `esdiag-local up` handles the clipboard failure
- **THEN** startup and browser launch continue
- **AND** the command prints platform-appropriate manual guidance without revealing the password

### Requirement: Bounded Failure Handling
Readiness and setup waits SHALL use bounded timeouts. When startup fails, `esdiag-local` MUST exit non-zero, retain persistent state, capture relevant service logs, and provide a recovery command without automatically deleting volumes.

#### Scenario: Service readiness timeout
- **GIVEN** a required service does not become healthy before its timeout
- **WHEN** `esdiag-local up` reaches the timeout
- **THEN** the command exits non-zero
- **AND** records diagnostic logs in the managed state directory
- **AND** leaves persistent volumes intact

### Requirement: Repository-Based Custom Builds
`esdiag-control` SHALL remain repository-dependent and retain source-based `build` and `buildx` workflows. Its local lifecycle commands SHALL use the canonical `esdiag-local` orchestration. `esdiag-control up` MUST set `ESDIAG_IMAGE_TAG=esdiag:${version}` and disable ESDiag image pulling so the repository-built image is used for setup and service execution.

#### Scenario: Starting a source-built deployment
- **GIVEN** a user is in an ESDiag source checkout and must build an auditable local image
- **WHEN** the user executes `esdiag-control up`
- **THEN** `esdiag-control` builds or selects the repository-derived ESDiag image
- **AND** delegates stack orchestration to `esdiag-local` with `ESDIAG_IMAGE_TAG=esdiag:${version}`
- **AND** the ESDiag image is not pulled from a remote registry

### Requirement: Release Publication Ordering
The release process SHALL publish the standalone `esdiag-local` artifact only after the matching versioned multi-platform ESDiag image is available for all supported architectures. Each official GitHub release SHALL attach the version-pinned executable as `esdiag-local` and its checksum as `esdiag-local.sha256`. The release MUST remain a draft until image manifests, non-SNAPSHOT embedded versions, script validation, checksum generation, asset attachment, and attachment verification all succeed.

#### Scenario: Matching image is unavailable
- **GIVEN** a release script references an ESDiag image version whose required registry manifests are missing
- **WHEN** release publication validation runs
- **THEN** publication of the standalone script fails

#### Scenario: Release assets are attached
- **GIVEN** the matching ESDiag image manifests exist for every supported architecture
- **WHEN** the GitHub release action publishes a tagged release
- **THEN** the release contains an executable `esdiag-local` asset pinned to that release version
- **AND** contains a matching `esdiag-local.sha256` asset
- **AND** is published only after both attachments are verified

### Requirement: Checksum Trust Documentation
Release documentation SHALL explain that `esdiag-local.sha256` verifies artifact download integrity within the same GitHub repository trust boundary and does not provide independent publisher authentication or signing.

#### Scenario: User reviews checksum guidance
- **GIVEN** a user reads standalone installation or update documentation
- **WHEN** checksum verification is described
- **THEN** the documentation distinguishes integrity verification from independent authenticity

### Requirement: Stable Release Discovery
The project SHALL document `https://ela.st/esdiag-local` as the stable human-facing URL for the latest official ESDiag release. The URL SHALL resolve to `https://github.com/elastic/esdiag/releases/latest`, where the standalone script and checksum are available as release assets.

#### Scenario: Discovering the latest standalone release
- **GIVEN** a user navigates to `https://ela.st/esdiag-local`
- **WHEN** the redirect is followed
- **THEN** the user reaches the latest official `elastic/esdiag` GitHub release
- **AND** can download the `esdiag-local` and `esdiag-local.sha256` assets

### Requirement: Official Self-Update Check
`esdiag-local update --check` SHALL query releases from the official `https://github.com/elastic/esdiag` repository, compare the latest released script version with the running version, and report update availability without modifying the installed script or local stack state.

#### Scenario: Newer official release is available
- **GIVEN** the official ESDiag repository has a newer released `esdiag-local` artifact
- **WHEN** the user executes `esdiag-local update --check`
- **THEN** the command reports the running and available versions
- **AND** reports the official release artifact URL
- **AND** does not modify the installed script

#### Scenario: Running release is current
- **GIVEN** the running script version matches the latest official release
- **WHEN** the user executes `esdiag-local update --check`
- **THEN** the command reports that the script is current
- **AND** does not modify the installed script

### Requirement: Verified Atomic Self-Replacement
`esdiag-local update` SHALL use `curl` to download the `esdiag-local` and `esdiag-local.sha256` assets from the latest official `github.com/elastic/esdiag` release. It MUST verify the checksum and downloaded script identity before atomically replacing the resolved running-script path.

#### Scenario: Successful self-update
- **GIVEN** a newer official release and matching checksum are available
- **AND** the installed script and its parent directory permit replacement
- **WHEN** the user executes `esdiag-local update`
- **THEN** the artifact and checksum are downloaded with `curl`
- **AND** the checksum, shell syntax, command identity, and expected version are verified
- **AND** the new executable atomically replaces the installed script
- **AND** local stack configuration, credentials, containers, and volumes remain unchanged

#### Scenario: Download or verification fails
- **GIVEN** the release download is incomplete, unavailable, has an invalid checksum, or does not identify itself as the expected `esdiag-local` version
- **WHEN** the user executes `esdiag-local update`
- **THEN** the command exits non-zero
- **AND** the existing installed script remains unchanged and executable

#### Scenario: Installation path is not writable
- **GIVEN** the running script cannot replace its resolved installation path
- **WHEN** the user executes `esdiag-local update`
- **THEN** the command exits without modifying the installed script
- **AND** prints the official artifact URL and manual installation guidance

#### Scenario: Script is invoked through PATH
- **GIVEN** the installed regular file is invoked by command name through `PATH`
- **WHEN** the user executes `esdiag-local update`
- **THEN** the updater resolves the regular file to a safely quoted absolute path
- **AND** downloads and atomically replaces it from a temporary file in the same directory

#### Scenario: Script is invoked through a symbolic link
- **GIVEN** the running command path is a symbolic link
- **WHEN** the user executes `esdiag-local update`
- **THEN** automatic replacement is refused without changing the link or its target
- **AND** manual installation guidance is printed

#### Scenario: Script is sourced
- **GIVEN** `esdiag-local` was sourced rather than executed
- **WHEN** update behavior is requested
- **THEN** self-replacement is refused without modifying any executable

### Requirement: Opt-In Update Network Access
Ordinary `esdiag-local` lifecycle commands MUST NOT check GitHub for script updates or mutate the installed executable. Update network access and self-replacement SHALL occur only through the explicit `update` command.

#### Scenario: Starting without an update check
- **GIVEN** the installed script is older than the latest official release
- **WHEN** the user executes `esdiag-local up`
- **THEN** the command performs the requested local stack lifecycle without contacting GitHub for a script release
- **AND** the installed script is not replaced

### Requirement: Standalone and Delegation Test Coverage
The project SHALL maintain `tests/esdiag-local.sh` for standalone behavior and SHALL update `tests/bin/esdiag-control.sh` to cover repository image construction and delegated lifecycle behavior.

#### Scenario: Standalone containment regression test
- **GIVEN** the test runs the script from an isolated directory without repository assets in its working path
- **WHEN** the standalone integration suite executes the supported lifecycle
- **THEN** configuration generation, startup, setup, repeated startup, inspection, shutdown, and reset expectations are verified

#### Scenario: Custom-build delegation regression test
- **GIVEN** the control integration suite has built a repository-derived ESDiag image
- **WHEN** it exercises the delegated local lifecycle
- **THEN** `ESDIAG_IMAGE_TAG` selects the locally built image for the setup and service containers with pulling disabled
- **AND** the resulting Elasticsearch, Kibana, and ESDiag services pass their health and setup checks

#### Scenario: Self-update test isolation
- **GIVEN** the update suite has copied `esdiag-local` into a temporary installation directory and placed fixture-backed fake network and checksum tools first on `PATH`
- **WHEN** update success and failure cases execute
- **THEN** requests target the hard-coded official release URLs using fixture responses
- **AND** only the temporary script can be replaced
- **AND** the repository script remains unchanged

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
