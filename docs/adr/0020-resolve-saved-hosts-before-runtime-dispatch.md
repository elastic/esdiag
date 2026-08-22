---
type: Reference
title: "Resolve saved hosts before runtime dispatch"
status: accepted
tags: [repository, adr]
---

# Resolve saved hosts before runtime dispatch

A saved host records three independent facts: the target **application**, its endpoint
state (a concrete URL or a URL template), and its **route** (direct or Elastic Cloud
admin). ESDiag resolves those facts into a concrete target before it constructs a
client, receiver, or collector.

## Context

The legacy `Product` enum conflated application, platform, and host-routing concepts.
Although ADR-0001 replaced it with `Platform` and `Application` for diagnostics, saved
host records still used it as a compatibility input. That made an absent or `Unknown`
application ambiguous: it could mean an unresolved template, a platform value stored
in the wrong field, or a concrete record that requires correction.

Elastic Cloud admin endpoints add another distinction: they route to an Elasticsearch
application through a Cloud admin proxy. The route proves `ElasticCloudHosted` as a
platform hint, but it is not an application.

## Decision

- A saved host's `app` field holds only a real `Application`, or is absent for an
  unresolved dynamic template. It never contains a platform, route, or `Unknown`.
- A persisted `KnownHost` may be unresolved for compatibility and template storage,
  but only a resolved concrete host enters runtime dispatch.
- Resolution validates the concrete URL, target application, route, and host roles
  together. A dynamic template selects its application when materialized.
- Legacy host records remain readable: `Unknown`, `None`, and platform values in
  `app` normalize to no application. A concrete record with no unambiguous application
  fails at runtime with correction guidance rather than defaulting to Elasticsearch.
- Cloud admin routing is transport metadata. Its resolved target application is
  Elasticsearch, and its receiver supplies an `ElasticCloudHosted` platform hint.

## Alternatives considered

- **Add `Unknown`, platform, or Cloud admin variants to `Application`.** Rejected:
  those are not Stack applications, and doing so would recreate the flattened
  classification ADR-0001 removed.
- **Allow each caller to inspect optional host fields.** Rejected: it duplicates
  validation and allows unresolved hosts to enter dispatch paths.
- **Keep global `Product` solely for manifest compatibility.** Rejected: it leaves an
  appealing but incorrect general-purpose type available to new code. The manifest
  keeps its wire-compatible `product` representation at its own boundary.

## Consequences

- CLI and Web forms share one backend host-resolution path and present unresolved
  templates explicitly.
- Role validation applies to the resolved application, regardless of route.
- The general `Product` domain type can be retired; diagnostic manifests retain their
  additive-only `product` wire field under manifest-local compatibility handling.
