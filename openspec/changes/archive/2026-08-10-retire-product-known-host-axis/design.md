## Context

ADR-0001 replaced the diagnostic-wide `Product` classification with independent
`Platform` and optional `Application` axes, but kept `Product` as a transitional Rust
type. The remaining call sites mix three different facts:

- the application API being addressed (`Elasticsearch`, `Kibana`, or `Logstash`);
- how that API is reached (direct URL, Elastic Cloud admin proxy, or URL template);
- the deployment platform inferred for the resulting diagnostic.

`KnownHost` already stores an optional `Application`, but compatibility builders,
dispatch code, tests, and the manifest wire model still depend on `Product`. An absent
host application currently means several things: an unresolved template, a legacy
`Unknown` value, or a legacy platform value. Runtime code must not guess among those
states.

The saved-host schema is user-managed persisted data, while diagnostic manifests are
permanent interchange artifacts. Hosts may be normalized when rewritten; manifests
remain additive-only and are never migrated.

## Goals / Non-Goals

**Goals:**

- Give saved hosts an application axis independent of endpoint transport and platform
  provenance.
- Make the unresolved-to-concrete host transition explicit before constructing a
  client, receiver, or collector.
- Preserve legacy `hosts.yml` reads while ensuring current writes contain only valid
  application values or omit the application for an unresolved template.
- Remove the global `Product` Rust type without changing the legacy manifest wire
  contract.
- Use the same model and validation rules in core code, the CLI, and the Web UI.

**Non-Goals:**

- Removing, renaming, or repurposing the manifest `product` field.
- Adding live collection for Elastic Agent or platform diagnostics.
- Adding new template placeholders or changing the reference grammar introduced by
  issue 304.
- Inferring a diagnostic platform from a saved-host application. Platform detection
  remains receiver-owned and best-effort.
- Introducing a new receiver, processor, exporter, or external dependency.

## Decisions

### 1. Model application, endpoint state, and route independently

A saved host record has three independent concepts:

- `application: Option<Application>` identifies the application API and data selected;
- endpoint state is either a concrete URL or a URL template;
- route metadata identifies direct routing or a recognized Elastic Cloud admin route.

An Elastic Cloud admin URL is therefore not an application value. Once concrete, its
target application is Elasticsearch, while the route selects
`ElasticCloudAdminReceiver` and supplies an `ElasticCloudHosted` platform hint.

This preserves the ADR-0001 distinction: application answers "whose data?", route
answers "how is it reached?", and platform answers "where was it deployed?".

**Alternative considered:** introduce Cloud admin or template variants into
`Application`. Rejected because neither is an Elastic Stack application and doing so
would recreate the flattened `Product` axis.

### 2. Resolve persisted records before runtime dispatch

Keep a persisted `KnownHost` record capable of representing compatibility and template
states, but introduce a validated concrete boundary (for example,
`ResolvedKnownHost`) consumed by clients and receivers.

The state transition is:

```text
KnownHost record
  ├─ concrete URL + explicit/inferred application ─┐
  ├─ template + id + selected/default application ├─> ResolvedKnownHost
  └─ ambiguous legacy concrete record ─────────────┘   or validation error
```

`ResolvedKnownHost` always has a concrete URL and one API-collectable `Application`.
Template rendering validates placeholders, renders the URL, assigns the selected or
fixed application, derives route metadata, and validates roles before crossing the
boundary. An unresolved template or ambiguous legacy record cannot construct a runtime
client.

Receivers continue to implement their existing `Receive`/`ReceiveRaw` traits; this
change only narrows their constructor input. No new receiver or processor trait is
introduced.

**Alternative considered:** let every runtime caller inspect `Option<Application>` and
endpoint fields. Rejected because it duplicates validation and permits invalid states
to reach dispatch.

### 3. Normalize legacy saved-host application values at the wire boundary

Deserialization remains tolerant:

- legacy Elasticsearch/Kibana/Logstash values become the matching `Application`;
- `Unknown`, `None`, and legacy platform-only values become an absent application;
- a concrete endpoint may infer an application only from an unambiguous endpoint shape;
- otherwise the record remains readable but must be corrected before runtime use or
  rewrite.

Current serialization writes only an `Application` value for a classified target and
omits `app` for a genuinely unresolved template. It never writes `Unknown` or a
platform into `app`.

**Alternative considered:** add `Unknown` to `Application`. Rejected because
`Application` is a closed set of actual Stack components; uncertainty belongs to host
resolution state, not the application vocabulary.

### 4. Dispatch live collection by `Application`

Client, receiver, collector, source-registry, archive-name, and host-role decisions use
`Application` directly. Functions that support only live collection accept or validate
the API-collectable subset: Elasticsearch, Kibana, and Logstash. Elastic Agent remains
Load-only.

Platform values are used only for diagnostic provenance and platform-level bundle
handling. This makes an accidental platform/application match a compile-time modeling
error rather than another `Product` match arm.

### 5. Isolate permanent manifest compatibility

The manifest field remains named `product` and accepts every historical value. Its
Rust representation moves into the manifest compatibility module under a wire-specific
name such as `ManifestProduct`. That type converts at the manifest boundary into
`Platform` plus optional `Application` and is not exported as a general domain type.

This is not a manifest schema migration and does not change emitted or accepted wire
values.

**Alternative considered:** retain global `Product` solely for manifests. Rejected
because it remains available to unrelated code and allows the transition to regress.

### 6. Record the host classification vocabulary

Add saved-host, route, unresolved host, and resolved host terms to `CONTEXT.md`. Record
the persisted-record/runtime-boundary decision in an ADR because it governs a durable
schema, is non-obvious for Cloud admin routes, and has rejected alternatives.

## Risks / Trade-offs

- **Legacy concrete records can be readable but not runnable** → infer only from
  unambiguous endpoints and return actionable validation directing the user to set an
  application; never silently default an ambiguous legacy record.
- **A broad alias removal can accidentally alter manifest compatibility** → isolate
  manifest wire tests before deleting `Product`, including historical values and
  round-trip fixtures.
- **CLI and Web validation can drift** → centralize record resolution and role
  validation in `data::known_host`; surfaces submit records to the same boundary.
- **Dynamic templates defer some validation** → validate fixed constraints at save
  time and validate selected application, rendered URL, route, and roles atomically at
  materialization.
- **Renaming types creates a large mechanical diff** → migrate by semantic layer and
  keep exhaustive matches so the compiler identifies remaining call sites.

## Migration Plan

1. Add the persisted-record to resolved-host boundary and compatibility tests without
   changing the serialized host shape.
2. Move CLI and Web host creation, template materialization, URI resolution, and role
   validation onto that boundary.
3. Migrate dispatch and application-specific helpers from `Product` to `Application`.
4. Introduce the manifest-local compatibility type and prove historical manifest
   fixtures still deserialize and serialize unchanged.
5. Remove `Product`, its conversion helpers, compatibility builders, and remaining
   non-wire references.
6. Update domain documentation, user-facing host documentation, and the changelog.

Rollback can restore the internal alias because no irreversible persisted-data rewrite
or manifest schema change is introduced. Hosts written by this change use fields and
application values older compatible versions already understand.

## Open Questions

None. The specification fixes the blocking decision: Cloud admin is route metadata,
templates are unresolved records, and only a resolved concrete host crosses into
runtime dispatch.
