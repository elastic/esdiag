## ADDED Requirements

### Requirement: Auto-trigger host syscall phase in collect workflow
The collection execution workflow SHALL include a host syscall phase that is automatically evaluated during `collect` without requiring an additional CLI flag.

#### Scenario: Collect runs without extra syscall flag
- **WHEN** a user runs `collect` with standard arguments
- **THEN** the workflow evaluates syscall phase eligibility using collected API data and local-node matching
- **AND** applies syscall phase behavior based on successful node/process variable resolution

### Requirement: Preserve existing API execution workflow while adding syscall phase
The collection workflow MUST preserve existing API collection behavior and archive finalization while adding conditional host syscall execution.

#### Scenario: Syscall phase does not replace API execution
- **WHEN** the syscall phase is enabled by local Elasticsearch detection
- **THEN** the workflow still executes configured API collection steps
- **AND** finalizes one combined archive containing all collected outputs

### Requirement: Resolve runtime variables before templated syscall execution
The collection workflow SHALL resolve template variables required by `java` and `logs` command groups from matched `_nodes` metadata and PID-specific process inspection before executing templated commands.

#### Scenario: Execute templated java and logs commands
- **WHEN** the workflow begins syscall command execution after matching local identity to one `_nodes` entry
- **THEN** it resolves required placeholders (including `{{PID}}`, `{{LOGPATH}}`, and `{{JAVA_HOME}}`) from `_nodes` metadata plus process inspection for that PID
- **AND** executes rendered commands using resolved values
- **AND** skips unresolved commands with warning logs instead of aborting the workflow

### Requirement: Match local machine identity to `_nodes` before syscall execution
The collection workflow SHALL gather local hostname/network identity and match it against `_nodes.host` and `_nodes.ip` values to select the node context used for syscall variable resolution.

#### Scenario: Node match using host/IP data
- **WHEN** local host/network identity is available
- **AND** `_nodes` response contains multiple nodes
- **THEN** the workflow compares local identity with each node's `host` or `ip`
- **AND** selects one matching node context before rendering syscall templates

### Requirement: Route syscall collection through receiver raw path
The collection workflow SHALL execute syscall collection through receiver raw-data methods (for example `get_raw_by_path` / `save_raw`) so command outputs remain unprocessed text.

#### Scenario: Syscall command is executed by receiver
- **WHEN** the workflow schedules a syscall command entry
- **THEN** receiver raw retrieval flow is used to execute/capture command output
- **AND** output is persisted through raw save path without schema parsing

### Requirement: Differentiate syscall command sources with dedicated path type
The data-source path model MUST include a dedicated syscall command path discriminator (for example `PathType::SystemCall`) to separate command execution from file and URL retrieval.

#### Scenario: Receiver resolves syscall command source
- **WHEN** syscall command data is requested
- **THEN** the receiver resolves source handling through the syscall path discriminator
- **AND** does not treat syscall commands as `PathType::File` or `PathType::Url`
