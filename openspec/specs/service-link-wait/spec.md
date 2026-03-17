### Requirement: Service Link Synchronous Processing
The `/api/service_link` endpoint SHALL accept a `wait_for_completion` query parameter that controls whether processing occurs synchronously or asynchronously.

#### Scenario: Parameter absent defaults to async
- **WHEN** a POST request is made to `/api/service_link` without a `wait_for_completion` parameter
- **THEN** the system MUST return HTTP 201 with `{"link_id": <integer>}` and process asynchronously

#### Scenario: Parameter present with no value enables sync
- **WHEN** a POST request is made to `/api/service_link?wait_for_completion`
- **THEN** the system MUST process the diagnostic synchronously and return HTTP 200 on success

#### Scenario: Parameter set to true enables sync
- **WHEN** a POST request is made to `/api/service_link?wait_for_completion=true`
- **THEN** the system MUST process the diagnostic synchronously and return HTTP 200 on success

#### Scenario: Parameter set to false preserves async
- **WHEN** a POST request is made to `/api/service_link?wait_for_completion=false`
- **THEN** the system MUST return HTTP 201 with `{"link_id": <integer>}` and process asynchronously

### Requirement: Service Link Synchronous Response Shape
When `wait_for_completion` is true, the `/api/service_link` endpoint SHALL return the same diagnostic result shape as the synchronous `/api/api_key` response.

#### Scenario: Successful synchronous completion
- **WHEN** `wait_for_completion=true` and the diagnostic processes successfully
- **THEN** the response MUST be HTTP 200 with a JSON body containing `diagnostic_id` (string), `kibana_link` (string, empty string if `ESDIAG_KIBANA_URL` is not configured), and `took` (integer milliseconds)

#### Scenario: Processing failure in sync mode
- **WHEN** `wait_for_completion=true` and the diagnostic processing fails
- **THEN** the response MUST be HTTP 500 with `{"error": "<message>"}`

#### Scenario: Receiver creation failure in sync mode
- **WHEN** `wait_for_completion=true` and the receiver cannot be created from the upload service URI
- **THEN** the response MUST be HTTP 500 with `{"error": "Failed to create receiver: <detail>"}`

### Requirement: API Documentation Accuracy
The API documentation in `docs/api/` SHALL accurately reflect the implemented endpoint behavior, field names, and types.

#### Scenario: kibana_link field name in docs matches implementation
- **WHEN** the synchronous API key or service link response documentation is read
- **THEN** the response field MUST be named `kibana_link`, not `kibana_url`

#### Scenario: link_id type in docs matches implementation
- **WHEN** the `UploadServiceResponse` type documentation is read
- **THEN** `link_id` MUST be documented as `Integer`, not `String`

#### Scenario: case_number type consistency
- **WHEN** the `Identifiers` and `ApiKeyRequest` type documentation is read
- **THEN** `case_number` MUST be consistently documented as `String` (nullable) in all locations, matching the `Option<String>` type in `Identifiers`

#### Scenario: JSON examples are valid
- **WHEN** any JSON example in `docs/api/examples.md` is parsed
- **THEN** it MUST be valid JSON (no trailing commas, correct value types)

#### Scenario: No broken documentation links
- **WHEN** the `docs/api/README.md` references are followed
- **THEN** all linked files MUST exist

#### Scenario: File size limit accuracy
- **WHEN** the file size constraint documentation is read
- **THEN** the maximum upload size MUST be documented as 512 MiB (not 512 GiB)
