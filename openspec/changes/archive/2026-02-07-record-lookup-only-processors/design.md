## Context

The `DiagnosticReport` contains a `lookup` section that tracks metadata about enrichment tables. Currently, it only records the number of documents in each table. We need to expose the `parsed` status of these lookups to differentiate between an empty successful lookup and a failed one.

## Goals / Non-Goals

**Goals:**
- Include `parsed: bool` in each entry of the `lookup` section in the `DiagnosticReport`.
- Properly record failures (error count and failure list) for lookups that fail to parse.
- Maintain the separation between `lookup` (metadata tables) and `processor` (document producers).

**Non-Goals:**
- Add lookup metadata to the `processor` section.
- Change the structure of `ProcessorSummary`.

## Decisions

### 1. Update `LookupSummary` structure
Add `pub parsed: bool` to `LookupSummary` in `src/processor/diagnostic/report.rs`.

### 2. Update `add_lookup` logic
Modify `DiagnosticReport::add_lookup` to:
- Check the `parsed` status of the incoming `Lookup<T>`.
- If `parsed` is false, increment `diagnostic.lookup.errors` and push the lookup name to `diagnostic.lookup.failures`.
- Populate the `LookupSummary` with the `parsed` status.

### 3. Track status in `Lookup<T>`
Add a `parsed` field to the `Lookup<T>` struct in `src/processor/diagnostic/lookup.rs` and provide a `was_parsed()` builder method to mark successful initialization.

## Risks / Trade-offs

- **Risk**: Minimal. This is a additive change to the report JSON and utilizes existing failure-tracking patterns.
