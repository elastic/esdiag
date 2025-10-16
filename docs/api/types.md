# Data Types

This document defines the data types used by the ESDiag API.

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

Request payload for the `/api/service_link` endpoint.

```json
{
  "token": "string",
  "url": "string",
  "metadata": {
    "account": "string | null",
    "case_number": "number | null",
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
| `link_id` | String | Unique identifier for the created job |

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

### ApiKey Response (Synchronous)

Response from the `/api/api_key` endpoint when `wait_for_completion` is `true`.

```json
{
  "diagnostic_id": "string",
  "kibana_url": "string",
  "took": integer
}
```

| Field | Type | Description |
|-------|------|-------------|
| `diagnostic_id` | String | Unique identifier for the processed diagnostic |
| `kibana_url` | String | URL to view the diagnostic in Kibana dashboard (empty string if `ESDIAG_KIBANA_URL` is not configured) |
| `took` | Integer | Processing time in milliseconds |

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
- `201 Created` - Resource created successfully (used by `/api/api_key`)
- `400 Bad Request` - Invalid request data or parameters
- `422 Unprocessable Entity` - Invalid request data structure
- `500 Internal Server Error` - Server-side processing error

## Type Validation

### File Upload Constraints

- **File Extension**: Must be `.zip`
- **File Size**: Maximum 512 GiB (549,755,813,888 bytes)
- **Filename**: Must be provided and non-empty

## Notes

- All timestamps are in ISO 8601 format
- User identification is extracted from the `X-Goog-Authenticated-User-Email` header
- Optional fields may be `null` or omitted from responses
