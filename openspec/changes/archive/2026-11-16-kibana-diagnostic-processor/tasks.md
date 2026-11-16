## 1. Scaffolding

- [x] 1.1 Create `src/processor/kibana` module directory and `mod.rs`
- [x] 1.2 Register `kibana` module in `src/processor/mod.rs`
- [x] 1.3 Implement `KibanaMetadata` in `src/processor/kibana/metadata.rs` (extracting `diagnostic.*` and `node.*`)
- [x] 1.4 Implement `KibanaDiagnostic` struct and its `DiagnosticProcessor` trait

## 2. Core Processors

- [x] 2.1 Implement `kibana-node-stats` processor (kibana_stats.json)
- [x] 2.2 Implement `kibana-status` processor (kibana_status.json)
- [x] 2.3 Implement `kibana-settings` processor (kibana_fleet_settings.json, etc.)

## 3. Domain Processors

- [x] 3.1 Implement `kibana-security` processor (roles, users, actions)
- [x] 3.2 Implement `kibana-fleet` processor (agents, policies, packages)
- [x] 3.3 Implement `kibana-alerts` processor (alerts, health)
- [x] 3.4 Implement `kibana-spaces` processor
- [x] 3.5 Implement `kibana-task-manager` and `kibana-stack-monitoring` processors
- [x] 3.6 Implement `kibana-synthetics-uptime` and `kibana-detection-engine` processors

## 4. Integration & Validation

- [x] 4.1 Update `Diagnostic::try_new` to recognize Kibana products
- [x] 4.2 Update `Diagnostic::process` to route to `KibanaDiagnostic`
- [x] 4.3 Add integration test using `tests/archives/kibana-api-diagnostics-8.19.3.zip`

## 5. Verification Remediation

- [x] 5.1 Mark successfully parsed sources and persist read, parse, and export failures in diagnostic reports
- [x] 5.2 Process all root, space-scoped, and paginated source files, including legacy numbered filenames
- [x] 5.3 Register processable Kibana sources and honor `ProcessSelection`
- [x] 5.4 Include diagnostic version and collection timestamp in shared Kibana metadata
- [x] 5.5 Add minimal index templates for every emitted Kibana metrics stream
- [x] 5.6 Add direct tests for every requirement scenario, including scoped and paginated bundles
- [x] 5.7 Reconcile the design and runtime documentation with the implemented processor scope
- [x] 5.8 Pass formatting, Kibana processor, asset-contract, and strict OpenSpec validation checks

## 6. Review Remediation

- [x] 6.1 Report failed requested APIs from the manifest as diagnostic report warnings
- [x] 6.2 Keep files discovered elsewhere when a supported subtree is unreadable, and skip unrelated subtrees
- [x] 6.3 Record the Kibana space of space-scoped documents in `space.id`
- [x] 6.4 Treat sources the target Kibana version does not offer as unavailable
- [x] 6.5 Stage Kibana `Collect` + `Process` jobs without `Save` through a temporary bundle
- [x] 6.6 Locate archive bundle roots from the manifest instead of the first entry

## 7. Collection Compatibility

- [x] 7.1 Send `X-Elastic-Internal-Origin: Kibana` on Kibana collection requests
- [x] 7.2 Use the 9.0 replacement routes for actions, alerts, and Fleet and security packages
- [x] 7.3 Use `/api/fleet/agents/action_status` for agent upgrade status from Kibana 8.5
- [x] 7.4 Treat 404 responses for missing exception lists as absent sources
