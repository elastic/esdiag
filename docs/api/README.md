---
type: Reference
title: ESDiag API
description: Experimental HTTP API for submitting diagnostic processing jobs.
tags: [api, reference]
---

# ESDiag API

> [!WARNING]
> This API is experimental. Paths and response fields may change.

`esdiag serve` exposes the API. The base URL uses the server bind address and
port. The default local URL is `http://localhost:2501`.

Authentication follows the server's configured authentication provider. A local
user-mode server normally has no request authentication. A shared service
should sit behind its identity-aware proxy.

## Endpoints

| Method | Path | Result |
|---|---|---|
| `GET` | `/` | Web UI. Pass `?job_id=<ID>` to open a saved job. |
| `POST` | `/api/service_link` | Submit an Elastic Upload Service link. |
| `POST` | `/api/api_key` | Submit an Elasticsearch URL and API key. |

The two `POST` endpoints accept `wait_for_completion`:

| Value | Result |
|---|---|
| Omitted or `false` | Returns `201 Created` and a job ID. |
| `true` | Waits for processing and returns `200 OK` with result entries. |

Use `?wait_for_completion` or `?wait_for_completion=true` for synchronous
processing.

The request body limit is 512 MiB. Archive uploads accept `.zip` files.

## Request and response types

- [Types](types.md) defines request bodies and responses.
- [Examples](examples.md) shows one asynchronous and one synchronous request.

Do not send real credentials to an unauthenticated service. For normal local
use, prefer saved hosts, the encrypted keystore, and `esdiag process`.
