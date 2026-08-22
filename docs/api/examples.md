---
type: Reference
title: API examples
description: Example requests for the experimental ESDiag API.
tags: [api, examples, reference]
---

# API examples

The values below are placeholders. Do not paste real API keys or upload tokens
into a shell history or an unauthenticated service.

## Submit an upload link

This request returns immediately with a job ID:

```sh
curl --request POST http://localhost:2501/api/service_link \
  --header "Content-Type: application/json" \
  --data '{
    "token": "<upload-token>",
    "url": "https://upload.elastic.co/d/<upload-id>",
    "metadata": {
      "account": "customer-123",
      "case_number": "98765",
      "filename": "diagnostic.zip"
    }
  }'
```

```json
{
  "link_id": 456789
}
```

Open the web UI with the ID:

```text
http://localhost:2501/?link_id=456789
```

## Process an Elasticsearch endpoint

Add `wait_for_completion=true` when the caller needs the result in the HTTP
response:

```sh
curl --request POST \
  "http://localhost:2501/api/api_key?wait_for_completion=true" \
  --header "Content-Type: application/json" \
  --data '{
    "apikey": "<api-key>",
    "url": "https://elasticsearch.example.com",
    "metadata": {
      "account": "customer-123",
      "case_number": "98765"
    }
  }'
```

```json
[
  {
    "status": "success",
    "outcome": "complete",
    "diagnostic_id": "elasticsearch-diagnostic-2024-01-15-abc123",
    "kibana_link": "https://kibana.example.com/...",
    "took": 42000,
    "product": "Elasticsearch",
    "source": "parent"
  }
]
```

See [API types](types.md) for all fields and error responses.
