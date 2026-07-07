---
type: Reference
title: Data Types
description: Data type definitions used by the ESDiag service API.
tags: [api, types, reference]
---

# Data Types

This document defines the data types used by the ESDiag API.

## Core Types

### Identifiers

Metadata structure used to identify and categorize diagnostic bundles.

```json
{
  "account": "string | null",
  "case_number": "string | null",
  "filename": "string | null",
  "opportunity": "string | null",
  "user": "string | null"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `account` | String (optional) | Account identifier associated with the diagnostic |
| `case_number` | String (optional) | Case number for support ticket tracking |
| `filename` | String (optional) | Original filename of the diagnostic bundle |
| `opportunity` | String (optional) | Business opportunity identifier |
| `user` | String (optional) | User who created/uploaded the diagnostic |

### UploadServiceRequest

Request payload for the `/api/service_link` endpoint.

```json
{
  "token": "string",
  "url": "string",
  "metadata": {
    "account": "string | null",
    "case_number": "string | null",
    "filename": "string | null",
    "opportunity": "string | null",
    "user": "string | null"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `metadata` | Identifiers | Diagnostic metadata and identifiers |
| `token` | String | Authentication token for the external Elasticsearch service |
| `url` | String | URL of the external Elasticsearch service endpoint |

### Upload Service Response

Response from the `/api/service_link` endpoint.

```json
{
  "link_id": integer
}
```

| Field | Type | Description |
|-------|------|-------------|
| `link_id` | Integer | Unique identifier for the created job |

### ApiKeyRequest

Request payload for the `/api/api_key` endpoint.

```json
{
  "apikey": "string",
  "url": "string",
  "metadata": {
    "account": "string | null",
    "case_number": "string | null",
    "filename": "string | null",
    "opportunity": "string | null",
    "user": "string | null"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `metadata` | Identifiers | Diagnostic metadata and identifiers |
| `apikey` | String | API key for authenticating with the Elasticsearch cluster |
| `url` | String | URL of the Elasticsearch cluster endpoint |

### ApiKey Response (Asynchronous)

Response from the `/api/api_key` endpoint when `wait_for_completion` is `false` or not specified.

```json
{
  "key_id": integer
}
```

| Field | Type | Description |
|-------|------|-------------|
| `key_id` | Integer | Unique identifier for the created API key job |

### Synchronous Response (wait_for_completion=true)

Response from `/api/api_key` or `/api/service_link` when `wait_for_completion` is `true`.

```json
[
  {
    "status": "success",
    "diagnostic_id": "string",
    "kibana_link": "string",
    "took": 12345,
    "product": "Elasticsearch",
    "source": "parent"
  },
  {
    "status": "info",
    "product": "Kibana",
    "source": "included_diagnostic",
    "path": "namespace/kibana/instance",
    "reason": "Kibana processing is not yet implemented"
  }
]
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | String | `success`, `info`, or `failed` |
| `diagnostic_id` | String | Present for `status: "success"` entries. Unique identifier for a successfully processed diagnostic |
| `kibana_link` | String | Present for `status: "success"` entries. URL to view a successful diagnostic in Kibana dashboard (empty string if `ESDIAG_KIBANA_URL` is not configured) |
| `took` | Integer | Present for `status: "success"` entries. Processing time in milliseconds |
| `product` | String | Product associated with the result entry when known |
| `source` | String | `parent` or `included_diagnostic` |
| `path` | String | Present for `source: "included_diagnostic"` entries. Included diagnostic path for child entries |
| `reason` | String | Present for `status: "info"` entries. Explanation for informational skipped entries |
| `error` | String | Present for `status: "failed"` child entries. Error message for failed child entries |

### Error Response

Standard error response format used across all endpoints.

```json
{
  "error": "string",
}
```

| Field | Type | Description |
|-------|------|-------------|
| `error` | String | Human-readable error message |

### HTTP Status Codes

- `200 OK` - Request successful
- `201 Created` - Resource created successfully (used by `/api/service_link` and `/api/api_key` when `wait_for_completion=false`)
- `400 Bad Request` - Invalid request data or parameters
- `422 Unprocessable Entity` - Invalid request data structure
- `500 Internal Server Error` - Server-side processing error

## Type Validation

### File Upload Constraints

- **File Extension**: Must be `.zip`
- **File Size**: Maximum 512 MiB (536,870,912 bytes)
- **Filename**: Must be provided and non-empty

## Notes

- All timestamps are in ISO 8601 format
- User identification is extracted from the `X-Goog-Authenticated-User-Email` header
- Optional fields may be `null` or omitted from responses
