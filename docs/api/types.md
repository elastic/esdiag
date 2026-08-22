---
type: Reference
title: API types
description: Request and response shapes for the experimental ESDiag API.
tags: [api, types, reference]
---

# API types

The [API overview](README.md) describes the endpoints and request modes.

## Metadata

Both request types accept optional metadata:

```json
{
  "account": "string | null",
  "case_number": "string | null",
  "filename": "string | null",
  "opportunity": "string | null",
  "user": "string | null"
}
```

| Field | Meaning |
|---|---|
| `account` | Account identifier. |
| `case_number` | Support case number. |
| `filename` | Original archive name. |
| `opportunity` | Opportunity identifier. |
| `user` | Person who submitted the archive. |

## `POST /api/service_link`

Use this endpoint for an Elastic Upload Service link:

```json
{
  "token": "string",
  "url": "https://upload.elastic.co/d/...",
  "metadata": {}
}
```

`token` authenticates the upload link. `url` is the Elastic Upload Service URL.

An asynchronous request returns:

```json
{
  "link_id": 456789
}
```

## `POST /api/api_key`

Use this endpoint to process data from an Elasticsearch endpoint:

```json
{
  "apikey": "string",
  "url": "https://elasticsearch.example.com",
  "metadata": {}
}
```

An asynchronous request returns:

```json
{
  "key_id": 12345
}
```

## Synchronous result

Requests with `wait_for_completion=true` return an array of result entries:

```json
[
  {
    "status": "success",
    "outcome": "complete",
    "diagnostic_id": "string",
    "kibana_link": "string",
    "took": 12345,
    "product": "Elasticsearch",
    "source": "parent"
  }
]
```

| Field | Meaning |
|---|---|
| `status` | `success`, `info`, or `failed`. |
| `outcome` | Derived outcome, such as `complete`, `partial`, or `failed`. |
| `diagnostic_id` | ID for a processed diagnostic. |
| `kibana_link` | Kibana URL when the server has one. |
| `took` | Processing time in milliseconds. |
| `product` | Product name when known. |
| `source` | `parent` or `included_diagnostic`. |
| `path` | Included diagnostic path, when present. |
| `reason` | Reason for an informational result. |
| `error` | Error message for a failed result. |

## Errors

Errors use this shape:

```json
{
  "error": "message"
}
```

The API uses `400 Bad Request` for invalid values, `422 Unprocessable Entity`
for an invalid body, and `500 Internal Server Error` when processing fails.

Optional fields may be `null` or omitted.
