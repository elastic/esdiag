## MODIFIED Requirements

### Requirement: Derived Execution Mode
The system SHALL derive a job's execution mode from its stage selection rather than storing
it. A job that selects both `Save` and `Process` SHALL execute in **staged** mode, where
collection completes and the bundle materialises before processing reads it (the bundle is a
serialization barrier). A job that selects `Collect` and `Process` without `Save` SHALL
execute in **streaming** mode, where receive, transform, and export overlap concurrently,
except for Kibana: because only the Kibana collector enumerates paginated and space-scoped
responses, a Kibana job that selects `Collect` and `Process` without `Save` SHALL execute in
staged mode through a temporary bundle that is not retained. A single executor SHALL drive
both modes.

#### Scenario: Save plus process is staged
- **WHEN** a job selects `Collect`, `Save`, and `Process`
- **THEN** the executor MUST complete collection and materialise the bundle before processing begins

#### Scenario: Collect plus process without save is streaming
- **WHEN** a job for an application other than Kibana selects `Collect` and `Process` with no `Save`
- **THEN** the executor MUST overlap receiving, transforming, and exporting concurrently
- **AND** MUST NOT require an intermediate bundle to materialise first

#### Scenario: Kibana collect plus process without save is staged
- **WHEN** a Kibana job selects `Collect` and `Process` with no `Save`
- **THEN** the executor MUST collect every page and space into a temporary bundle before processing reads it
- **AND** MUST NOT retain the temporary bundle after the job

#### Scenario: One executor drives both modes
- **WHEN** a `Job` is executed, whether its derived mode is staged or streaming
- **THEN** it MUST run through the same executor, which selects its strategy from the derived mode
- **AND** no second executor or mode-specific operation type MUST exist for either mode
