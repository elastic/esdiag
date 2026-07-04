## MODIFIED Requirements

### Requirement: Service Link Synchronous Response Shape
When `wait_for_completion` is true, the `/api/service_link` endpoint SHALL return the same diagnostic result array shape as the synchronous `/api/api_key` response.

#### Scenario: Successful synchronous completion
- **WHEN** `wait_for_completion=true` and the diagnostic processes successfully
- **THEN** the response MUST be HTTP 200 with a JSON array body
- **AND** the array MUST contain at least one diagnostic result entry
- **AND** each successful diagnostic result entry MUST contain `status` (string), `diagnostic_id` (string), `kibana_link` (string, empty string if `ESDIAG_KIBANA_URL` is not configured), and `took` (integer milliseconds)

#### Scenario: Synchronous completion with included diagnostics
- **WHEN** `wait_for_completion=true` and the diagnostic is an ECK or KubernetesPlatform parent bundle with included diagnostic outcomes
- **THEN** the response MUST be HTTP 200 with a JSON array body
- **AND** the array MUST contain one entry for the parent diagnostic
- **AND** the array MUST contain one entry for each included diagnostic outcome

#### Scenario: Synchronous completion with skipped included diagnostic
- **WHEN** `wait_for_completion=true` and an included diagnostic is recognized but skipped because processing is not implemented
- **THEN** the response MUST be HTTP 200 with a JSON array body
- **AND** the skipped diagnostic result entry MUST contain `status: "info"` and a reason string

#### Scenario: Synchronous completion with failed included diagnostic
- **WHEN** `wait_for_completion=true`, parent processing succeeds, and an included diagnostic fails
- **THEN** the response MUST be HTTP 200 with a JSON array body
- **AND** the failed child diagnostic result entry MUST contain `status: "failed"` and an error string
- **AND** the parent diagnostic result entry MUST remain present

#### Scenario: Processing failure in sync mode
- **WHEN** `wait_for_completion=true` and the parent diagnostic processing fails
- **THEN** the response MUST be HTTP 500 with `{"error": "<message>"}`

#### Scenario: Receiver creation failure in sync mode
- **WHEN** `wait_for_completion=true` and the receiver cannot be created from the upload service URI
- **THEN** the response MUST be HTTP 500 with `{"error": "Failed to create receiver: <detail>"}`
