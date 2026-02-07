## Why

Currently, the `DiagnosticReport` records document counts for lookups (enrichment tables) in the `lookup` section, but it does not capture whether those lookups were successfully parsed. If a lookup processor fails, enrichment may be missing or incorrect without any clear indication in the report.

## What Changes

- **Enhance Lookup Metadata**: Add a `parsed` boolean to the `lookup` summary section in the diagnostic report.
- **Record Failures**: Populate the `errors` and `failures` fields of the `lookup` section when a lookup data source fails to parse.
- **Maintain Separation**: Keep lookup status within the `lookup` section rather than adding it to the `processor` section, avoiding confusion between metadata tables and document producers.

## Capabilities

### Modified Capabilities

- `diagnostic-reporting`: Updated the `LookupSummary` structure and the `add_lookup` logic to capture and report parsing status and failures.

## Impact

- `src/processor/diagnostic/report.rs`: Updated `LookupSummary` and `DiagnosticReport::add_lookup`.
- `src/processor/diagnostic/lookup.rs`: Added `parsed` flag to `Lookup<T>` to track initialization status.
- `DiagnosticReport` output: The `lookup` section now includes `"parsed": true/false` for each entry, and tracks failure counts and names.
