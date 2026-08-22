## MODIFIED Requirements

### Requirement: Child Diagnostic Completion Links
Each successfully processed included diagnostic SHALL be reported with its own diagnostic metadata and Kibana link.

#### Scenario: Supported child diagnostic completes
- **WHEN** an included Elasticsearch diagnostic completes successfully
- **THEN** the child job result MUST display that child report's `diagnostic.id`
- **AND** the child job result MUST link to that child report's Kibana URL when a Kibana base URL is configured
- **AND** the child job result MUST display the child report's product, created document count, and processing duration

#### Scenario: Multiple supported children complete
- **WHEN** multiple included Elasticsearch diagnostics complete from the same parent bundle
- **THEN** the job feed MUST display one completed result per child diagnostic
- **AND** each completed result MUST use the `diagnostic.id` and Kibana link from its own child report

#### Scenario: CLI process returns structured child outcomes
- **WHEN** the CLI `process` command completes an ECK or KubernetesPlatform parent bundle with one or more successfully processed child diagnostics
- **THEN** the structured process outcome MUST include one completed child entry per diagnostic
- **AND** each entry MUST contain its `diagnostic.id`, product, created document count, integer duration in milliseconds, and Kibana URL when configured
- **AND** the outcome MUST NOT present the empty parent diagnostic link as the only actionable result

### Requirement: Unsupported Included Diagnostic Info Results
Recognized included diagnostics without an implemented diagnostic processor SHALL be reported as informational skipped child results rather than hidden or failed parent work.

#### Scenario: Unsupported child diagnostic is recognized
- **WHEN** an included diagnostic manifest is readable but its product does not have an implemented diagnostic processor
- **THEN** the job feed MUST display an `info` status result for that child diagnostic
- **AND** the result MUST explain that the child diagnostic was recognized but skipped because processing is not implemented

#### Scenario: CLI process returns structured skipped child
- **WHEN** the CLI `process` command reads an included diagnostic manifest whose product does not have an implemented processor
- **THEN** the structured process outcome MUST include a skipped child entry
- **AND** the entry MUST contain the source path, product when known, and skip reason

#### Scenario: API reports unsupported child diagnostic
- **WHEN** synchronous API processing reads an included diagnostic manifest whose product does not have an implemented processor
- **THEN** the API result array MUST include an entry for that child diagnostic with `status: "info"`
- **AND** the entry MUST explain that the child diagnostic was recognized but skipped because processing is not implemented

#### Scenario: Unsupported children do not block supported children
- **WHEN** a parent bundle contains both supported Elasticsearch child diagnostics and recognized unsupported child diagnostics
- **THEN** supported child diagnostics MUST still process and render completed results
- **AND** unsupported child diagnostics MUST render informational skipped results

## ADDED Requirements

### Requirement: Failed Included Diagnostics Remain Typed CLI Children
A failed included diagnostic SHALL appear as a failed child entry inside an otherwise successful parent CLI process outcome. The entry SHALL contain the source path and safe error message and MUST NOT convert the entire parent command into a failed outcome when the parent processing lifecycle completed successfully.

#### Scenario: Child fails after parent succeeds
- **GIVEN** parent diagnostic processing completes successfully
- **WHEN** one included diagnostic fails
- **THEN** the process command exits successfully with a structured parent outcome
- **AND** the failed child appears with a failed discriminator, source path, and safe error message
- **AND** completed or skipped sibling outcomes remain present
