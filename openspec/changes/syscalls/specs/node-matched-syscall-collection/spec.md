## ADDED Requirements

### Requirement: Resolve local node context from `_nodes`
The system SHALL resolve local node context for syscall collection by matching local machine identity against `_nodes` response entries.

#### Scenario: Match local machine to node by host or IP
- **WHEN** local identity is gathered (hostname, interface display names, interface IPs, interface hostnames, canonical hostnames)
- **AND** `_nodes` contains node `host` and `ip` fields
- **THEN** the system matches local identity to one node using node `host`/`ip` comparisons

### Requirement: Use first-match order for node tie-breaks
The system SHALL use deterministic first-match order when multiple `_nodes` entries satisfy local identity matching.

#### Scenario: Multiple nodes satisfy host/IP comparison
- **WHEN** more than one node matches local identity via `host` or `ip`
- **THEN** the first matching node in traversal order is selected as syscall context

### Requirement: Derive `PID` and `LOGPATH` from matched node metadata
The system SHALL derive `{{PID}}` from `nodes.<id>.process.id` and `{{LOGPATH}}` from `nodes.<id>.settings.path.logs` of the matched node.

#### Scenario: Matched node contains runtime metadata
- **WHEN** a node is selected from `_nodes`
- **THEN** `{{PID}}` resolves from that node's `process.id`
- **AND** `{{LOGPATH}}` resolves from that node's `settings.path.logs`

### Requirement: Derive `JAVA_HOME` from process command line of resolved PID
The system SHALL derive `{{JAVA_HOME}}` by inspecting `ps -ef` (or OS equivalent) for the resolved PID and taking the first executable path in the process description.

#### Scenario: Process command line contains bundled Java path
- **WHEN** the PID from `_nodes` is used to inspect process details
- **THEN** the first executable path in the process command line is used to derive `{{JAVA_HOME}}`

### Requirement: Execute OS-specific syscall commands with existing template format
The system SHALL execute syscall commands from `assets/elasticsearch/syscalls.yml` for the active OS and SHALL preserve `{{VARIABLE}}` placeholder syntax.

#### Scenario: Render and execute templated commands
- **WHEN** commands include placeholders such as `{{PID}}`, `{{LOGPATH}}`, and `{{JAVA_HOME}}`
- **THEN** placeholders are rendered from node/process-derived values
- **AND** commands are executed for the selected OS sections

### Requirement: Capture syscall outputs as raw receiver data
The system MUST capture syscall command outputs as unprocessed text through receiver raw-data interfaces.

#### Scenario: Syscall command output is collected
- **WHEN** a syscall command completes
- **THEN** its output is captured and persisted through raw-data save flow without parsing into structured API documents

### Requirement: Merge syscall outputs into the existing diagnostic bundle
The system MUST write syscall command outputs into the same archive bundle produced for API collection.

#### Scenario: Bundle contains API and syscall outputs
- **WHEN** syscall commands are executed during `collect`
- **THEN** their outputs are added to the existing diagnostic archive
- **AND** no additional archive artifact is created for syscall data

### Requirement: Keep syscall command failures non-fatal
The system SHALL continue collection when syscall commands fail or timeout and SHALL emit warning logs for those failures.

#### Scenario: A syscall command fails or times out
- **WHEN** execution of one syscall command returns an error or exceeds timeout
- **THEN** the system logs a warning for that command
- **AND** continues with remaining syscall commands and API collection
