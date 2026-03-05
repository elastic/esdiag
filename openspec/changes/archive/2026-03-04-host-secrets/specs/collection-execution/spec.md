## ADDED Requirements

### Requirement: Role-Constrained Execution Targets
The collection execution workflow SHALL resolve host targets by role before executing each workflow phase. The collect phase SHALL use only hosts with the `collect` role, the send phase SHALL use only hosts with the `send` role, and the view phase SHALL use only hosts with the `view` role.

#### Scenario: Resolve targets for multi-phase workflow
- **GIVEN** host configuration includes hosts with `collect`, `send`, and `view` roles
- **WHEN** the workflow resolves targets for collection and output handling
- **THEN** collection calls are executed only against `collect` hosts
- **AND** send/output calls are executed only against `send` hosts
- **AND** view target resolution includes only `view` hosts
