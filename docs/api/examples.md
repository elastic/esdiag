# API Usage Examples

This document provides practical examples of how to use the ESDiag API endpoints.

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
    <title>ESDiag</title>
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
  "link_id": 456789
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

## POST `/api/api_key` - API Key Processing

### Request
```bash
curl -X POST http://localhost:3000/api/api_key \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "account": "Acme, Inc.",
      "case_number": "98765",
      "opportunity": null,
      "user": "user@example.com"
    },
    "apikey": "abcdefghijklmnopqrstuvwxyz=",
    "url": "https://elasticsearch.example.com"
  }'
```

### Successful Response
```json
{
  "key_id": 12345
}
```

### Error Response - Invalid URL
```json
{
  "error": "Failed to parse URL: relative URL without a base"
}
```

### Error Response - Empty API Key
```json
{
  "error": "API key cannot be empty"
}
```

### Error Response - Host Build Error
```json
{
  "error": "Failed to build host: unsupported host type"
}
```

## Complete Workflow Examples

### Example: Basic service link forwarding workflow

1. Push upload service link to ESDiag

```bash
curl -X POST http://localhost:3000/api/service_link \
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

### Example: API key workflow

1. Submit API key and Elasticsearch URL to ESDiag

```bash
curl -X POST http://localhost:3000/api/api_key \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "account": "Acme, Inc",
      "case_number": "12345",
      "user": "user@example.com"
    },
    "apikey": "abcdefghijklmnopqrstuvwxyz=",
    "url": "https://my-cluster.es.example.com"
  }'
```

2. Retrieve `key_id` from response

```json
{ "key_id": 12345 }
```

3. Forward user to ESDiag with `key_id` as a parameter

```bash
open "http://localhost:3000/?key_id=12345"
```
