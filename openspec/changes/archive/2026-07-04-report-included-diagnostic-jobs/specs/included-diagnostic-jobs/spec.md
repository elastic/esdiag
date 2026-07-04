## ADDED Requirements

### Requirement: Included Diagnostic Job Fan-Out Reporting
When a web processing job receives an ECK or KubernetesPlatform parent diagnostic bundle with `included_diagnostics`, the system SHALL report each included diagnostic as a distinct child job in the web job feed.

#### Scenario: Parent bundle starts multiple child jobs
- **WHEN** a web processing job starts for an ECK or KubernetesPlatform parent bundle whose manifest lists multiple `included_diagnostics`
- **THEN** the job feed MUST display a separate progress box for each included diagnostic that esdiag starts processing
- **AND** each child progress box MUST identify the child diagnostic source or path separately from the parent bundle source

#### Scenario: Parent bundle does not hide child work
- **WHEN** the parent ECK or KubernetesPlatform processor completes without producing orchestration-level documents
- **THEN** the job feed MUST preserve the parent result
- **AND** the job feed MUST show child diagnostic job results when child outcomes exist

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

#### Scenario: CLI process reports child links
- **WHEN** the CLI `process` command completes an ECK or KubernetesPlatform parent bundle with one or more successfully processed child diagnostics
- **THEN** the CLI summary MUST include each completed child diagnostic's `diagnostic.id`
- **AND** the CLI summary MUST include each completed child diagnostic's Kibana link when a Kibana base URL is configured
- **AND** the CLI summary MUST NOT present the empty parent diagnostic link as the only actionable result

### Requirement: Synchronous API Multi-Result Reporting
Synchronous diagnostic processing APIs SHALL return one JSON array entry for each diagnostic result produced by processing.

#### Scenario: API returns parent and child diagnostic results
- **WHEN** synchronous `/api/api_key` or `/api/service_link` processing completes an ECK or KubernetesPlatform parent bundle with included diagnostic outcomes
- **THEN** the HTTP 200 response body MUST be a JSON array
- **AND** the array MUST contain an entry for the parent diagnostic
- **AND** the array MUST contain one entry for each included diagnostic outcome

#### Scenario: API successful diagnostic entry
- **WHEN** a parent or child diagnostic processes successfully in a synchronous API request
- **THEN** that diagnostic result entry MUST include `status: "success"`
- **AND** that diagnostic result entry MUST include `diagnostic_id`, `kibana_link`, and `took`

#### Scenario: API child failure entry does not fail parent response
- **WHEN** parent processing succeeds but an included diagnostic fails
- **THEN** the synchronous API response MUST remain HTTP 200
- **AND** the failed child diagnostic result entry MUST include `status: "failed"` and an error message
- **AND** the parent diagnostic result entry MUST remain present

### Requirement: Unsupported Included Diagnostic Info Results
Recognized included diagnostics without an implemented diagnostic processor SHALL be reported as informational skipped child results rather than hidden or failed parent work.

#### Scenario: Unsupported child diagnostic is recognized
- **WHEN** an included diagnostic manifest is readable but its product does not have an implemented processor
- **THEN** the job feed MUST display an `info` status result for that child diagnostic
- **AND** the result MUST explain that the child diagnostic was recognized but skipped because processing is not implemented

#### Scenario: CLI process reports unsupported child diagnostic
- **WHEN** the CLI `process` command reads an included diagnostic manifest whose product does not have an implemented processor
- **THEN** the CLI summary MUST include an informational skipped entry for that child diagnostic
- **AND** the skipped entry MUST explain that the child diagnostic was recognized but skipped because processing is not implemented

#### Scenario: API reports unsupported child diagnostic
- **WHEN** synchronous API processing reads an included diagnostic manifest whose product does not have an implemented processor
- **THEN** the API result array MUST include an entry for that child diagnostic with `status: "info"`
- **AND** the entry MUST explain that the child diagnostic was recognized but skipped because processing is not implemented

#### Scenario: Unsupported children do not block supported children
- **WHEN** a parent bundle contains both supported Elasticsearch child diagnostics and recognized unsupported child diagnostics
- **THEN** supported child diagnostics MUST still process and render completed results
- **AND** unsupported child diagnostics MUST render informational skipped results

### Requirement: Child Outcome Preservation
The processing lifecycle SHALL preserve structured outcomes for included diagnostics so callers can distinguish completed, skipped, and failed child work.

#### Scenario: Processor completes parent with child outcomes
- **WHEN** a processor completes an ECK or KubernetesPlatform parent bundle with included diagnostics
- **THEN** the completed processor state MUST expose the parent diagnostic report
- **AND** the completed processor state MUST expose a child outcome for every included diagnostic that was started, skipped, or failed

#### Scenario: Child failure does not fail completed parent
- **WHEN** parent processing succeeds and a child diagnostic fails
- **THEN** the completed processor state MUST expose the child failure as a child outcome
- **AND** the parent processor MUST still complete successfully

#### Scenario: Parent with skipped or no children succeeds
- **WHEN** an ECK or KubernetesPlatform parent diagnostic has no included diagnostics or all included diagnostics are skipped
- **THEN** the parent processor MUST still complete successfully

#### Scenario: Child report keeps parent relationship
- **WHEN** a child diagnostic is processed from a parent bundle
- **THEN** the child diagnostic report MUST retain the parent diagnostic relationship metadata required by the orchestration metadata capability

#### Scenario: Included diagnostic reporting remains one level deep
- **WHEN** a child diagnostic contains its own `included_diagnostics`
- **THEN** this capability MUST NOT require recursive multi-level reporting

### Requirement: Default Child Processing Selection
Included diagnostics SHALL process with their default product processor selection.

#### Scenario: Parent processing has included diagnostics with different products
- **WHEN** an ECK or KubernetesPlatform parent bundle contains included diagnostics for one or more products
- **THEN** each included diagnostic MUST use its default product processing selection
- **AND** parent-level process selection MUST NOT be applied as a filter to child diagnostics
