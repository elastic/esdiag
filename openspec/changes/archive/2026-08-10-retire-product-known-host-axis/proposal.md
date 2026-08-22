## Why

`Product` remains a flattened compatibility type after ADR-0001 split diagnostic
classification into `Platform` and `Application`. Its remaining use in saved hosts,
receiver/collector dispatch, and application-specific helpers preserves ambiguous states
such as `Product::Unknown` for unresolved templates and platform values in an `app`
field, preventing the transitional alias from being retired.

## What Changes

- Define known-host classification on independent axes: an optional target
  `Application`, a concrete or template transport, and transport-specific metadata.
- Treat a template-backed host as unresolved rather than as an unknown application;
  resolving a template reference produces a concrete host with a supported application.
- Treat Elastic Cloud admin routing as transport metadata. A concrete Cloud admin proxy
  target still identifies the application whose APIs and diagnostic data are selected.
- Replace application and dispatch uses of `Product` with `Application`; use `Platform`
  only for deployment provenance.
- Preserve read compatibility for legacy saved hosts whose `app` contains `Unknown` or
  a platform value, while newly written records omit `app` when unresolved and serialize
  only `Application` values when resolved.
- Remove the global Rust `Product` type after all non-wire call sites migrate.
- Keep the legacy manifest field named `product` and its accepted wire values unchanged,
  using a manifest-local compatibility representation rather than the retired domain
  alias.
- Update CLI and Web host management to present an application only where the host is
  concrete or the template constrains it.

## Capabilities

### New Capabilities

- `known-host-classification`: Defines the independent application, transport, and
  template-resolution semantics of saved host records, including legacy read
  compatibility.

### Modified Capabilities

- `cli-host-record-management`: Clarify application handling for concrete,
  template-backed, and materialized-template host definitions.
- `host-role-targeting`: Validate role compatibility against a concrete target
  application without treating transport or an unresolved template as an application.
- `web-hosts-keychain-manager`: Render and validate the same host classification model
  as the CLI when creating or editing saved hosts.

## Impact

- **Target products:** Elasticsearch, Kibana, and Logstash live collection targets;
  Elastic Agent remains Load-only.
- **Core:** `data::KnownHost`, URI classification, clients, receivers, collectors,
  collection source selection, setup/export helpers, and the legacy manifest parser.
- **CLI:** saved-host add/update/list/auth flows and application flags.
- **Web UI:** host forms, host rows, validation, and Datastar state for concrete and
  template-backed hosts.
- **Persistence:** `hosts.yml` remains backward-readable; current writes normalize the
  host application axis. Diagnostic manifest wire compatibility remains additive-only
  and unchanged.
