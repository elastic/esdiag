## Context

ESDiag currently has placeholders for Kibana diagnostic processing but lacks implementation. This design outlines the structure for adding Kibana-specific processors, following the established patterns used by Elasticsearch and Logstash processors.

## Goals / Non-Goals

**Goals:**
- Implement `KibanaDiagnostic` in `src/processor/kibana`.
- Register Kibana in `Diagnostic::try_new` and `Diagnostic::process`.
- Implement processors for the comprehensive set of Kibana diagnostic files (node stats, settings, status, security, fleet, alerts, spaces, task manager, stack monitoring, synthetics, uptime, and detection engine).
- Ensure Kibana documents are sent to appropriate data streams (e.g., `metrics-kibana.node-esdiag`).

**Non-Goals:**
- Implementing Kibana dashboards (installed separately by `esdiag setup`).
- Support for legacy (pre-7.x) Kibana diagnostic formats unless they follow current API patterns.

## Decisions

- **Module Structure**: Create `src/processor/kibana/` with `mod.rs` defining `KibanaDiagnostic` and one subdirectory per spec capability, following the Logstash diagnostic processor pattern. A source belongs to the capability whose spec names it, so `kibana_fleet_settings` and `kibana_uptime_settings` live in `settings/` rather than `fleet/` or `synthetics_uptime/`.
- **Document Shape**: Payload fields are placed at the top level of each document and shared metadata is applied last, so payload keys such as `@timestamp`, `node`, `diagnostic`, or `data_stream` cannot replace document identity. List responses produce one document per item, and non-object items are kept under `value`. Documents from `spaces/<space>/` files carry `space.id`, because space-scoped responses such as connectors do not name their space.
- **Trait Implementation**: Use `DiagnosticProcessor` for the main `KibanaDiagnostic` and `DocumentExporter` for individual API processors.
- **Single Node Workflow**: Treat Kibana diagnostics as a single-node workflow, similar to Logstash, as it is representative of Kibana deployments.
- **Data Model**: Flatten and enrich Kibana JSON outputs with standard ESDiag metadata.
- **Data Streams**: Use names following the pattern `<type>-kibana-esdiag` or `metrics-kibana.<type>-esdiag` to maintain consistency with other products.
- **Source Discovery**: Consume every file emitted for a source, including root files, space-scoped files, paginated files, and legacy numbered filenames. Preserve deterministic path order when combining pages and spaces.
- **Selection**: Register each implemented source in the shared processing registry and dispatch only the sources selected for processing, while always loading metadata prerequisites.
- **Outcome Reporting**: Mark decoded sources as parsed and persist non-missing read, parse, and export failures in the diagnostic report. Missing optional sources remain non-fatal, but failed requests recorded in the manifest's `requested_apis` become report warnings, as ADR-0016 requires. An unreadable supported subtree is reported as a failure of that source without discarding files found elsewhere. Sources the target Kibana version does not offer are unavailable, not failed.
- **Live Processing**: Kibana `Collect` + `Process` jobs without `Save` collect into a temporary bundle before processing, because live receivers return one response per API and only the Kibana collector enumerates pages and spaces.
- **Collection Compatibility**: Collection requests declare an internal origin so Kibana 9 serves internal routes. Sources whose legacy routes were removed use the replacement routes from support-diagnostics at 9.0; `current_upgrades` uses `action_status` from 8.5, ahead of upstream. Missing exception lists are absent data, not failures.
- **Metadata**: Enrich every document with the diagnostic identifier, diagnostic version, collection timestamp, Kibana node identity and version, and data stream identity.
- **Index Templates**: Add an index template for every emitted metrics stream. Explicit mappings cover only dashboard-critical metadata and stable node fields; dynamic mappings capture product payloads that can vary between Kibana versions.

## Risks / Trade-offs

- **[Risk]** Kibana diagnostic API responses can change between versions.
  - **Mitigation** Use `serde_json::Value` for initial parsing and gracefully handle missing or unexpected fields.
- **[Trade-off]** Implementing every possible Kibana API would be high effort.
  - **Decision** Implement the capability set listed in this change while leaving unlisted Kibana APIs for follow-up changes.
- **[Trade-off]** Explicitly mapping every Kibana response field would make templates brittle across versions.
  - **Decision** Map the metadata required by the first dashboard and allow dynamic templates to capture the remaining payload.
