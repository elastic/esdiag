# malformed-ip-normalization Specification

## Purpose
TBD - created by archiving change normalize-malformed-scrubbed-ips. Update Purpose after archive.
## Requirements
### Requirement: Receiver applies scrub normalization before processors
The system SHALL normalize scrubbed archive payloads in the receiver read path before any processor consumes file content.

#### Scenario: Scrubbed archive in process flow
- **WHEN** an archive input is classified as scrubbed
- **THEN** receiver returns normalized content to processors
- **AND** processors execute without requiring scrub-specific mutation logic

#### Scenario: Non-scrubbed archive in process flow
- **WHEN** an archive input is classified as non-scrubbed
- **THEN** receiver returns original content unchanged

### Requirement: Normalization is bounded to supported files
The system SHALL apply receiver-stage normalization only to supported JSON/text diagnostic files that contain node address and node identity surfaces.

#### Scenario: Supported file
- **WHEN** receiver reads a supported file (for example `nodes.json` or `nodes_stats.json`)
- **THEN** receiver applies scrub normalization rules before returning content

#### Scenario: Unsupported file
- **WHEN** receiver reads an unsupported file type
- **THEN** receiver SHALL pass content through unchanged

### Requirement: Scrub mode supports auto and manual controls
The system SHALL support implicit auto mode plus explicit control in both CLI and upload channels.

#### Scenario: Auto mode detection
- **WHEN** `--scrubbed` is not provided and archive filename/path contains `scrubbed`
- **THEN** scrub normalization SHALL be enabled

#### Scenario: Auto mode no match
- **WHEN** `--scrubbed` is not provided and archive filename/path does not contain `scrubbed`
- **THEN** scrub normalization SHALL remain disabled

### Requirement: Manual override precedence
The system SHALL apply manual selection precedence within the active execution channel.

#### Scenario: Manual on overrides auto no-match
- **WHEN** `--scrubbed true` is provided
- **THEN** normalization SHALL run regardless of filename/path

#### Scenario: Manual off overrides auto match
- **WHEN** `--scrubbed false` is provided and filename/path contains `scrubbed`
- **THEN** normalization SHALL NOT run

### Requirement: CLI and UI are independent execution channels
The system SHALL treat CLI process flow and UI upload flow as either/or channels without cross-channel override behavior.

#### Scenario: CLI channel
- **WHEN** operator uses CLI process flow
- **THEN** scrub behavior SHALL be controlled by `--scrubbed BOOL` or implicit auto mode

#### Scenario: UI channel
- **WHEN** operator uses upload UI flow
- **THEN** scrub behavior SHALL be controlled by checkbox or auto mode within UI flow

### Requirement: Deterministic malformed IPv4 normalization
The system SHALL normalize malformed scrubbed IPv4 octets using deterministic per-octet modulo transformation (`octet % 255`).

#### Scenario: Invalid scrubbed IPv4 value
- **WHEN** a scrubbed address value contains octets outside valid IPv4 range
- **THEN** each octet SHALL be rewritten via modulo 255
- **AND** the rewritten value SHALL be deterministic for the same input

### Requirement: Field-allowlist safety boundary
The system SHALL apply malformed IP normalization only to explicitly supported address fields.

#### Scenario: Allowed address field
- **WHEN** a malformed IPv4 value appears in an allowed address field
- **THEN** normalization SHALL be applied

#### Scenario: Non-address field
- **WHEN** a malformed IPv4-like string appears in a non-address field
- **THEN** normalization SHALL NOT be applied

### Requirement: Port and pure-IP semantics
The system SHALL preserve port components for address fields that allow ports and SHALL store pure IP values for fields with pure IP semantics.

#### Scenario: Address with port in transport field
- **WHEN** an allowed transport field contains `ip:port`
- **THEN** IP component SHALL be normalized
- **AND** original port SHALL be preserved

#### Scenario: Pure IP field
- **WHEN** a pure IP field (`ip` or `host`) contains `ip:port` shaped scrubbed data
- **THEN** output SHALL contain normalized IP without port

### Requirement: Scrubbed node name detection
The system SHALL detect scrubbed node-name inputs matching 19-character lowercase hex format.

#### Scenario: 19-char lowercase hex name
- **WHEN** node name is a 19-character lowercase hexadecimal string
- **THEN** system SHALL classify it as scrubbed-name format

#### Scenario: Non-matching name
- **WHEN** node name does not match 19-character lowercase hexadecimal format
- **THEN** system SHALL treat it as non-scrubbed for this rename rule

### Requirement: Last-4 readable rename default
The system SHALL use existing tier rename logic shape but replace numeric segment with last 4 characters of original scrubbed node name.

#### Scenario: Scrubbed name with tier
- **WHEN** a scrubbed 19-character lowercase hexadecimal node name is processed for tier `hot`
- **THEN** output name SHALL use tier prefix `hot` and suffix equal to the last four characters of the source name (for example, `…99e7` → `hot-99e7`)

#### Scenario: Non-scrubbed existing behavior
- **WHEN** node name uses existing `instance-...` pattern
- **THEN** current rename behavior SHALL remain unchanged

### Requirement: Normalization emits debug logs per file
The system SHALL emit debug logs for scrub normalization decisions and transformations.

#### Scenario: Normalization enabled
- **WHEN** scrub normalization runs for an archive
- **THEN** logs SHALL include mode source and transformed field counts

#### Scenario: File-level normalization logging
- **WHEN** receiver reads a file that is unscrubbed/normalized
- **THEN** system SHALL emit a debug log line for that file read and transformation action

#### Scenario: Manual override
- **WHEN** manual scrub mode overrides auto detection
- **THEN** logs SHALL record effective mode and override source

### Requirement: Non-mangling verification logs
The system SHALL log verification outcomes for non-scrubbed pass-through behavior in test and validation flows.

#### Scenario: Non-scrubbed fixture validation
- **WHEN** validation runs against non-scrubbed fixtures
- **THEN** logs SHALL indicate unchanged pass-through result

### Requirement: Development workflow SHALL validate live ingest cleanliness
The development workflow SHALL include a test path that runs `esdiag process --debug` against an Elasticsearch target and verifies that ingest completes without bulk/index mapping conflicts.

#### Scenario: Clean ingest run
- **WHEN** a scrubbed diagnostic fixture is processed to a live Elasticsearch dev target
- **THEN** bulk ingest SHALL complete with no conflict/rejection entries

#### Scenario: Conflict detection gate
- **WHEN** any bulk rejection, mapping conflict, or parse/index conflict occurs during ingest
- **THEN** the development validation step SHALL fail

### Requirement: Development workflow SHALL include artifact checks
The development workflow SHALL inspect esdiag run artifacts to confirm clean ingest outcomes.

#### Scenario: Last-run artifact verification
- **WHEN** development ingest validation runs in debug mode
- **THEN** it SHALL check `~/.esdiag/last_run` outputs for bulk errors and failed responses
- **AND** it SHALL fail if conflict indicators are present

#### Scenario: Node dataset sanity verification
- **WHEN** development ingest validation runs successfully
- **THEN** it SHALL verify node-related datasets are present (for example `metrics-node-esdiag`) and non-empty

### Requirement: Validation gates SHALL follow esdiag test tiers
The feature validation plan SHALL use deterministic gates for each implementation phase and treat environment-dependent integration tests as separately classified checks.

#### Scenario: Deterministic phase gate
- **WHEN** a phase changes receiver/server/processor logic
- **THEN** `cargo clippy --workspace --all-targets` and targeted deterministic tests SHALL pass before phase signoff

#### Scenario: Environment-dependent workspace tests
- **WHEN** `cargo test --workspace` includes tests requiring preconfigured hosts, local services, or container runtime
- **THEN** results SHALL be classified as environment-gated baseline vs new regression
- **AND** only new regressions introduced by this feature SHALL fail phase signoff

