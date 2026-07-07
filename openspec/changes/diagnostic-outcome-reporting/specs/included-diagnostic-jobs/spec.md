## MODIFIED Requirements

### Requirement: Child Outcome Preservation
The processing lifecycle SHALL preserve a `DiagnosticOutcome` for each included
diagnostic — the same first-class outcome type used for the parent — so callers can
distinguish `Complete`, `Partial`, `Failed`, and `Skipped` child work. A child's
outcome SHALL be derived from that child's own recorded report events exactly as the
parent's is (replacing the former child-only `IncludedDiagnosticOutcome`). A child
`Skipped` outcome SHALL indicate whether the skip was by-design or not-implemented.

#### Scenario: Processor completes parent with child outcomes
- **WHEN** a processor completes an ECK or KubernetesPlatform parent bundle with
  included diagnostics
- **THEN** the completed processor state MUST expose the parent diagnostic report
- **AND** the completed processor state MUST expose a `DiagnosticOutcome` for every
  included diagnostic that was started, skipped, or failed

#### Scenario: Child failure does not fail completed parent
- **WHEN** parent processing succeeds and a child diagnostic fails
- **THEN** the completed processor state MUST expose the child's `Failed` outcome
- **AND** the parent processor MUST still complete successfully

#### Scenario: Child completes with partial captures
- **WHEN** an included Elasticsearch diagnostic processes but at least one of its
  sources records a failure or partial-capture event
- **THEN** the child's `DiagnosticOutcome` MUST be `Partial`
- **AND** the parent processor MUST still complete successfully

#### Scenario: Parent with skipped or no children succeeds
- **WHEN** an ECK or KubernetesPlatform parent diagnostic has no included diagnostics
  or all included diagnostics are skipped
- **THEN** the parent processor MUST still complete successfully

#### Scenario: Child report keeps parent relationship
- **WHEN** a child diagnostic is processed from a parent bundle
- **THEN** the child diagnostic report MUST retain the parent diagnostic relationship
  metadata required by the orchestration metadata capability

#### Scenario: Included diagnostic reporting remains one level deep
- **WHEN** a child diagnostic contains its own `included_diagnostics`
- **THEN** this capability MUST NOT require recursive multi-level reporting
