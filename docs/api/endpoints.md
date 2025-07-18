# API Endpoints

This document provides detailed documentation for all ESdiag API endpoints.

## GET `/`

Serves the main application interface (HTML page).

### Request
- **Method:** GET
- **Authentication:** Optional (X-Goog-Authenticated-User-Email header)
- **Content-Type:** Not applicable

### Response
- **Content-Type:** text/html
- **Status Code:** 200 OK

Returns the main HTML interface for the application.

---

## GET `/status`

Get current processing status, queue information, and user job history.

### Request
- **Method:** GET
- **Authentication:** Optional (X-Goog-Authenticated-User-Email header)
- **Content-Type:** Not applicable

### Response
- **Content-Type:** application/json

#### Success Response (200 OK)

The response varies based on queue size:

**Ready State (queue empty):**
```json
{
  "status": "ready",
  "exporter": "exporter_name",
  "kibana": "kibana_url",
  "user": "user@example.com",
  "current": null,
  "queue": {
    "size": 0
  },
  "history": []
}
```

**Processing State (1-9 jobs in queue):**
```json
{
  "status": "processing",
  "progress": "Processing diagnostic...",
  "kibana": "kibana_url",
  "user": "user@example.com",
  "current": "current_job_info",
  "queue": {
    "size": 3
  },
  "history": []
}
```

**Busy State (10+ jobs in queue):**
```json
{
  "status": "busy",
  "warning": "Too many jobs in queue",
  "kibana": "kibana_url",
  "user": "user@example.com",
  "current": "current_job_info",
  "queue": {
    "size": 15
  },
  "history": []
}
```

#### Response Fields
| Field | Type | Description |
|-------|------|-------------|
| `status` | String | Current system status: "ready", "processing", or "busy" |
| `exporter` | String | Name of the configured exporter |
| `kibana` | String | Kibana URL for result viewing |
| `user` | String | Authenticated user email |
| `current` | Object/null | Currently processing job information |
| `queue.size` | Number | Number of jobs in processing queue |
| `history` | Array | User's job history (filtered to current user) |
| `progress` | String | Processing progress message (when status is "processing") |
| `warning` | String | Warning message (when status is "busy") |

---

## POST `/upload`

Upload a diagnostic bundle (.zip file) for processing.

### Request
- **Method:** POST
- **Authentication:** Optional (X-Goog-Authenticated-User-Email header)
- **Content-Type:** multipart/form-data
- **Body:** Multipart form with file field

#### Form Fields
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `file` | File | Yes | ZIP file containing diagnostic bundle |

#### File Requirements
- Must be a `.zip` file
- Maximum size: 1 GiB
- Must have a valid filename

### Response
- **Content-Type:** application/json

#### Success Response (200 OK)
```json
{
  "status": "processing",
  "message": "Received upload: filename.zip (12345 bytes)"
}
```

#### Error Responses

**400 Bad Request** - Invalid file type:
```json
{
  "error": "Invalid file type. Only .zip files are allowed."
}
```

**400 Bad Request** - No filename provided:
```json
{
  "error": "No file name provided"
}
```

**400 Bad Request** - No file in request:
```json
{
  "status": "error",
  "error": "No file part in the request"
}
```

**500 Internal Server Error** - Processing failure:
```json
{
  "status": "error",
  "error": "Failed to process the upload"
}
```

---

## POST `/upload_service`

Initiate diagnostic processing via an external Elasticsearch service URL.

### Request
- **Method:** POST
- **Authentication:** Optional (X-Goog-Authenticated-User-Email header)
- **Content-Type:** application/json

#### Request Body
```json
{
  "metadata": {
    "account": "account_name",
    "case_number": 12345,
    "filename": "diagnostic.zip",
    "opportunity": "opportunity_id",
    "user": "user@example.com"
  },
  "token": "elasticsearch_token",
  "url": "https://elasticsearch-instance.com/_path"
}
```

#### Request Fields
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `metadata` | Object | Yes | Diagnostic metadata identifiers |
| `metadata.account` | String | No | Account identifier |
| `metadata.case_number` | Number | No | Case number |
| `metadata.filename` | String | No | Original filename |
| `metadata.opportunity` | String | No | Opportunity identifier |
| `metadata.user` | String | No | User identifier (auto-filled from auth header) |
| `token` | String | Yes | Authentication token for Elasticsearch service |
| `url` | String | Yes | Elasticsearch service URL |

### Response
- **Content-Type:** application/json

#### Success Response (200 OK)
```json
{
  "status": "processing",
  "job_id": "unique_job_identifier",
  "queue_size": 3
}
```

#### Error Responses

**400 Bad Request** - Invalid URL:
```json
{
  "error": "Invalid URL: <error_details>"
}
```

**400 Bad Request** - Token setup failure:
```json
{
  "error": "Failed to set token in URL"
}
```

**500 Internal Server Error** - Service creation failure:
```json
{
  "error": "Failed to create receiver: <error_details>"
}
```

**500 Internal Server Error** - Job preparation failure:
```json
{
  "error": "Failed to prepare job: <error_details>"
}
```

#### Response Fields
| Field | Type | Description |
|-------|------|-------------|
| `status` | String | Processing status, typically "processing" |
| `job_id` | String | Unique identifier for the created job |
| `queue_size` | Number | Current number of jobs in the processing queue |

---

## Common Headers

### Required Headers
- `X-Goog-Authenticated-User-Email`: User's authenticated email address (set by Google IAP)

### Response Headers
All JSON responses include:
- `Content-Type: application/json`
- Standard HTTP status code headers

---

## Rate Limiting

Currently, there is no explicit rate limiting implemented, but the system has natural limits:
- Processing queue capacity: 10 jobs
- Body size limit: 1 GiB per request
- Jobs are processed sequentially to manage system resources
