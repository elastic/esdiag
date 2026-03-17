ESDiag API Documentation
========================

> [!WARNING]
> The API is not stable and is still a work-in-progress

This directory contains the API documentation for the ESDiag service, which processes Elastic Stack diagnostic bundles.

## Overview

The ESDiag API provides endpoints for:
- Accessing the web interface
- Checking processing status
- Uploading diagnostic bundles via form submission
- Initiating remote download from the Elastic Upload service (https://upload.elastic.co)

## Authentication

All endpoints require authentication through Google's Identity-Aware Proxy (IAP).

## Base URL

The API runs on the configured port (default port `2501`) and accepts requests at:

```
http://localhost:{port}
```

## External Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Serves the main application interface, may include a `?job_id=<job_id>` parameter to immediately start processing |
| POST | `/api/service_link` | Stores a `link_id` for later processing, or processes synchronously with `?wait_for_completion` |
| POST | `/api/api_key` | Stores a `key_id` for later processing, or processes synchronously with `?wait_for_completion` |

## Query Parameters

### `/api/api_key` and `/api/service_link` Endpoints

Both endpoints support the `wait_for_completion` parameter:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `wait_for_completion` | Boolean | `false` | When `true`, processes the diagnostic synchronously and returns the result. Can be specified as `?wait_for_completion`, `?wait_for_completion=true`, or `?wait_for_completion=false` |

When `wait_for_completion=true`:
- The request blocks until processing completes
- Returns `diagnostic_id`, `kibana_link`, and `took` (processing time in milliseconds)
- Returns HTTP 200 on success instead of 201
- May take significantly longer to respond depending on diagnostic size

When `wait_for_completion=false` (default):
- Returns immediately with a job ID (`key_id` for `/api/api_key`, `link_id` for `/api/service_link`)
- Processing occurs asynchronously in the background
- User must navigate to the web interface with the job ID to monitor progress

## Request Limits

- Maximum request body size: 512 MiB
- Supported file types: `.zip` files only

## Response Format

All API responses return JSON with consistent structure:
- Success responses include relevant data with appropriate status codes
  - 200 OK for most successful operations
  - 201 Created for resource creation endpoints
  - 400 Bad Request for invalid data content
  - 422 Unprocessable Entity for invalid data structure
- Error responses include an `error` field with descriptive message

## Documentation Structure

- [`types.md`](./types.md) - Data type definitions
- [`examples.md`](./examples.md) - Request/response examples
