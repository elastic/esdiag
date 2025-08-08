# API Usage Examples

This document provides practical examples of how to use the ESdiag API endpoints.

## Authentication

All requests read the `X-Goog-Authenticated-User-Email` header, which is typically set automatically by Google's Identity-Aware Proxy (IAP).

```bash
# Example header (usually set by IAP)
X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com
```

## GET `/` - Main Interface

### Request
```bash
curl -X GET http://localhost:3000/
```

### Response
```html
<!DOCTYPE html>
<html>
<head>
    <title>Elastic Stack Diagnostics</title>
</head>
<body>
    <!-- Main application interface -->
</body>
</html>
```

## POST `/api/service_link` - Remote Service Processing

### Request
```bash
curl -X POST http://localhost:3000/api/service_link \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com" \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "account": "customer-123",
      "case_number": 98765,
      "filename": "remote-diagnostic.zip",
      "opportunity": null,
    },
    "token": "0123456789",
    "url": "https://upload.elastic.co/d/abcdefghijklmnopqrstuvwxyz"
  }'
```

### Successful Response
```json
{
  "link_id": "job-456789"
}
```

### Error Response - URL not from Elastic Upload Service
```json
{
  "error": "URL must be for the Elastic Upload Service"
}
```

### Error Response - Missing token
```json
{
  "error": "Failed to set token in URL"
}
```

### Error Response - Empty Token
```json
{
  "error": "Authorization token cannot be empty"
}
```

## Complete Workflow Examples

### Example: Basic service link forwarding workflow

1. Push upload service link to ESDiag

```bash
curl -X POST http://localhost:3000/api/service_link \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com" \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "account": "customer-123",
      "case_number": "98765",
      "filename": "remote-diagnostic.zip",
      "opportunity": null
    },
    "token": "0123456789",
    "url": "https://upload.elastic.co/d/abcdefghijklmnopqrstuvwxyz"
  }'
```

2. Retrieve `link_id` from response
```json
{ "link_id": "45678" }
```

3. Forward user to ESDiag with `link_id` as a parameter
```bash
open "http://localhost:3000/?link_id=45678"
```
