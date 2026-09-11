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
