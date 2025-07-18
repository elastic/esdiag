# Data Types

This document defines the data types used by the ESdiag API.

## Core Types

### Identifiers

Metadata structure used to identify and categorize diagnostic bundles.

```json
{
  "account": "string | null",
  "case_number": "number | null",
  "filename": "string | null",
  "opportunity": "string | null",
  "user": "string | null"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `account` | String (optional) | Account identifier associated with the diagnostic |
| `case_number` | Number (optional) | Case number for support ticket tracking |
| `filename` | String (optional) | Original filename of the diagnostic bundle |
| `opportunity` | String (optional) | Business opportunity identifier |
| `user` | String (optional) | User who created/uploaded the diagnostic |

### UploadServiceRequest

Request payload for the `/upload_service` endpoint.

```json
{
  "metadata": {
    "account": "string | null",
    "case_number": "number | null",
    "filename": "string | null",
    "opportunity": "string | null",
    "user": "string | null"
  },
  "token": "string",
  "url": "string"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `metadata` | Identifiers | Diagnostic metadata and identifiers |
| `token` | String | Authentication token for the external Elasticsearch service |
| `url` | String | URL of the external Elasticsearch service endpoint |

### Job

Represents a processing job in the system.

```json
{
  "id": "string",
  "status": "string",
  "created_at": "string",
  "updated_at": "string",
  "identifiers": {
    "account": "string | null",
    "case_number": "number | null",
    "filename": "string | null",
    "opportunity": "string | null",
    "user": "string | null"
  },
  "error": "string | null"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | String | Unique identifier for the job |
| `status` | String | Current job status (e.g., "pending", "processing", "completed", "failed") |
| `created_at` | String | ISO 8601 timestamp when job was created |
| `updated_at` | String | ISO 8601 timestamp when job was last updated |
| `identifiers` | Identifiers | Associated metadata for the job |
| `error` | String (optional) | Error message if job failed |

### JobState

Internal state management for job processing.

```json
{
  "current": "Job | null",
  "history": ["Job[]"],
  "queue": ["Job[]"]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `current` | Job (optional) | Currently processing job, null if none |
| `history` | Array of Job | Completed jobs history (up to 100 entries) |
| `queue` | Array of Job | Pending jobs queue (up to 10 entries) |

## Response Types

### Upload Response

Response from the `/upload` endpoint.

```json
{
  "status": "string",
  "message": "string"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | String | Processing status, typically "processing" |
| `message` | String | Descriptive message about the upload |

### Status Response

Response from the `/status` endpoint.

```json
{
  "status": "string",
  "exporter": "string",
  "kibana": "string",
  "user": "string",
  "current": "Job | null",
  "queue": {
    "size": "number"
  },
  "history": ["Job[]"],
  "progress": "string | undefined",
  "warning": "string | undefined"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | String | System status: "ready", "processing", or "busy" |
| `exporter` | String | Name of the configured exporter |
| `kibana` | String | Kibana URL for viewing results |
| `user` | String | Authenticated user email |
| `current` | Job (optional) | Currently processing job |
| `queue.size` | Number | Number of jobs in processing queue |
| `history` | Array of Job | User's job history (filtered to current user) |
| `progress` | String (optional) | Progress message (present when status is "processing") |
| `warning` | String (optional) | Warning message (present when status is "busy") |

### Upload Service Response

Response from the `/upload_service` endpoint.

```json
{
  "status": "string",
  "job_id": "string",
  "queue_size": "number"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | String | Processing status, typically "processing" |
| `job_id` | String | Unique identifier for the created job |
| `queue_size` | Number | Current size of the processing queue |

### Error Response

Standard error response format used across all endpoints.

```json
{
  "error": "string",
  "status": "string | undefined"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `error` | String | Human-readable error message |
| `status` | String (optional) | Error status, typically "error" |

## Enums and Constants

### Job Status Values

- `"pending"` - Job is queued but not yet started
- `"processing"` - Job is currently being processed
- `"completed"` - Job completed successfully
- `"failed"` - Job failed with an error

### System Status Values

- `"ready"` - System is idle and ready to accept new jobs
- `"processing"` - System is processing jobs (1-9 jobs in queue)
- `"busy"` - System is heavily loaded (10+ jobs in queue)

### HTTP Status Codes

- `200 OK` - Request successful
- `400 Bad Request` - Invalid request data or parameters
- `500 Internal Server Error` - Server-side processing error

## Type Validation

### File Upload Constraints

- **File Extension**: Must be `.zip`
- **File Size**: Maximum 1 GiB (1,073,741,824 bytes)
- **Filename**: Must be provided and non-empty

### Queue Limits

- **Processing Queue**: Maximum 10 jobs
- **History**: Maximum 100 completed jobs stored
- **Channel Buffer**: 1 job buffer for upload processing

## Notes

- All timestamps are in ISO 8601 format
- User identification is automatically extracted from the `X-Goog-Authenticated-User-Email` header
- Job history and processing status is filtered per user
- Optional fields may be `null` or omitted from responses
