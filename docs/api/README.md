# ESdiag API Documentation

This directory contains the API documentation for the ESDiag service, which processes Elastic Stack diagnostic bundles.

## Overview

The ESdiag API provides endpoints for:
- Accessing the web interface
- Checking processing status
- Uploading diagnostic bundles via form submission
- Initiating remote download from the Elastic Upload service (https://upload.elastic.co)

## Authentication

All endpoints optionally read user information from `X-Goog-Authenticated-User-Email` header, which is set by Google's Identity-Aware Proxy (IAP).

## Base URL

The API runs on the configured port (default port `3000`) and accepts requests at:

```
http://localhost:{port}
```

## Available Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Serves the main application interface |
| GET | `/status` | Get current processing status and job queue information |
| POST | `/upload` | Upload diagnostic bundle for processing |
| POST | `/upload_service` | Initiate processing via external Elasticsearch service |

## Request Limits

- Maximum request body size: 1 GiB
- Supported file types: `.zip` files only

## Response Format

All API responses return JSON with consistent structure:
- Success responses include relevant data and status information
- Error responses include an `error` field with descriptive message
- Status codes follow HTTP standards

## Documentation Structure

- [`endpoints.md`](./endpoints.md) - Detailed endpoint documentation
- [`types.md`](./types.md) - Data type definitions
- [`examples.md`](./examples.md) - Request/response examples
- [`errors.md`](./errors.md) - Error handling and status codes

## Getting Started

1. Ensure you have proper Google authentication configured
2. Start the ESdiag service on your desired port
3. Use the `/status` endpoint to verify the service is running
4. Upload diagnostic bundles via the `/upload` endpoint
5. Monitor processing status and history via the `/status` endpoint

For detailed usage examples, see [`examples.md`](./examples.md).
