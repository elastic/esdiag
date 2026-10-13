## Context

ESDiag currently has placeholders for Kibana diagnostic processing but lacks implementation. This design outlines the structure for adding Kibana-specific processors, following the established patterns used by Elasticsearch and Logstash processors.

## Goals / Non-Goals

**Goals:**
- Implement `KibanaDiagnostic` in `src/processor/kibana`.
- Register Kibana in `Diagnostic::try_new` and `Diagnostic::process`.
- Implement processors for the comprehensive set of Kibana diagnostic files (node stats, settings, status, security, fleet, alerts, spaces, task manager, stack monitoring, synthetics, uptime, and detection engine).
- Ensure Kibana documents are sent to appropriate data streams (e.g., `metrics-kibana.node-esdiag`).

**Non-Goals:**
- Implementing dashboards or index templates (handled by `esdiag setup`).
- Support for legacy (pre-7.x) Kibana diagnostic formats unless they follow current API patterns.

## Decisions

- **Module Structure**: Create `src/processor/kibana/` with `mod.rs` defining `KibanaDiagnostic` and subdirectories for each capability, following the Logstash diagnostic processor pattern.
- **Trait Implementation**: Use `DiagnosticProcessor` for the main `KibanaDiagnostic` and `DocumentExporter` for individual API processors.
- **Single Node Workflow**: Treat Kibana diagnostics as a single-node workflow, similar to Logstash, as it is representative of Kibana deployments.
- **Data Model**: Flatten and enrich Kibana JSON outputs with standard ESDiag metadata.
- **Data Streams**: Use names following the pattern `<type>-kibana-esdiag` or `metrics-kibana.<type>-esdiag` to maintain consistency with other products.
- **Source Discovery**: Consume every file emitted for a source, including root files, space-scoped files, paginated files, and legacy numbered filenames. Preserve deterministic path order when combining pages and spaces.
- **Selection**: Register each implemented source in the shared processing registry and dispatch only the sources selected for processing, while always loading metadata prerequisites.
- **Outcome Reporting**: Mark decoded sources as parsed and persist non-missing read, parse, and export failures in the diagnostic report. Missing optional sources remain non-fatal.
- **Metadata**: Enrich every document with the diagnostic identifier, diagnostic version, collection timestamp, Kibana node identity and version, and data stream identity.
- **Index Templates**: Add an index template for every emitted metrics stream. Explicit mappings cover only dashboard-critical metadata and stable node fields; dynamic mappings capture product payloads that can vary between Kibana versions.

## Risks / Trade-offs

- **[Risk]** Kibana diagnostic API responses can change between versions.
  - **Mitigation** Use `serde_json::Value` for initial parsing and gracefully handle missing or unexpected fields.
- **[Trade-off]** Implementing every possible Kibana API would be high effort.
  - **Decision** Implement the capability set listed in this change while leaving unlisted Kibana APIs for follow-up changes.
- **[Trade-off]** Explicitly mapping every Kibana response field would make templates brittle across versions.
  - **Decision** Map the metadata required by the first dashboard and allow dynamic templates to capture the remaining payload.
