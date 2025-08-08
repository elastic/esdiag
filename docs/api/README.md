# ESdiag API Documentation

This directory contains the API documentation for the ESDiag service, which processes Elastic Stack diagnostic bundles.

## Overview

The ESdiag API provides endpoints for:
- Accessing the web interface
- Checking processing status
- Uploading diagnostic bundles via form submission
- Initiating remote download from the Elastic Upload service (https://upload.elastic.co)

## Authentication

All endpoints require authentication through Google's Identity-Aware Proxy (IAP).

## Base URL

The API runs on the configured port (default port `3000`) and accepts requests at:

```
http://localhost:{port}
```

## External Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Serves the main application interface, may include a `?job_id=<job_id>` parameter to immediately start processing |
| POST | `/api/service_link` | Stores a job_id for later processing |

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

- [`endpoints.md`](./endpoints.md) - Detailed endpoint documentation
- [`types.md`](./types.md) - Data type definitions
- [`examples.md`](./examples.md) - Request/response examples
- [`errors.md`](./errors.md) - Error handling and status codes
