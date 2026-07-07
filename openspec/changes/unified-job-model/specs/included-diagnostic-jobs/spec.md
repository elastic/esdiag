## ADDED Requirements

### Requirement: Included Diagnostics Execute as Child Jobs
The system SHALL execute each included diagnostic of a parent bundle as a **child `Job`**
driven by the same executor. When the executor processes a parent bundle whose manifest lists
`included_diagnostics`, each included diagnostic SHALL be executed as a child `Job`:
a `Load`-input job over the nested diagnostic plus a `Process` stage. Each child `Job` SHALL
mint its own child `JobID` identifying that one child execution. Child jobs SHALL reuse the
one executor rather than a separate processing type, and the parent SHALL set each child's
`Platform` as it spawns it. Fan-out SHALL remain one level deep — a child job's own
`included_diagnostics` SHALL NOT trigger recursive child-job spawning.

#### Scenario: Parent spawns a child job per included diagnostic
- **WHEN** the executor processes an ECK or KubernetesPlatform parent bundle whose manifest lists included diagnostics
- **THEN** the executor MUST spawn one child `Job` for each included diagnostic
- **AND** each child `Job` MUST be a `Load`-input, `Process` job driven by the same executor
- **AND** each child `Job` MUST mint its own child `JobID`

#### Scenario: Child job inherits parent platform
- **WHEN** the parent spawns a child `Job` for an included Elasticsearch diagnostic
- **THEN** the parent MUST set the child job's `Platform` from the parent as it spawns it

#### Scenario: Child job fan-out stays one level deep
- **WHEN** an included diagnostic processed as a child `Job` itself contains `included_diagnostics`
- **THEN** the executor MUST NOT recursively spawn grandchild jobs for that nested inclusion
