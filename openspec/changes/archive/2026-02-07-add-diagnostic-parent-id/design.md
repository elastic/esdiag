## Context

Currently, `esdiag` processes diagnostic bundles from various sources, including orchestration platforms like ECK and ECE. These platforms often produce "meta-bundles" that contain multiple component diagnostics. While the `DiagnosticManifest` (from `diagnostic_manifest.json`) already includes an `included_diagnostics` field, the resulting metadata does not explicitly link children back to their parent, nor does it consistently categorize the orchestration layer.

## Goals / Non-Goals

**Goals:**
- Enrich `DiagnosticMetadata` with `parent_id` and `orchestration` fields.
- Update `DiagnosticManifest` to hold and propagate parent-child context.
- Ensure the `orchestration` field is correctly populated based on the manifest's product type.

**Non-Goals:**
- Changing the structure of `diagnostic_manifest.json` itself.
- Retroactively updating existing indexed metadata (this applies to new processing jobs).

## Decisions

### 1. Update `DiagnosticMetadata` Struct
Add `parent_id: Option<String>` and `orchestration: Option<String>` to `src/processor/diagnostic/doc.rs`.
- **Rationale**: These fields are essential for downstream filtering and relationship mapping in Elasticsearch.
- **Alternatives**: Storing these in a separate object. Decision: Flat structure is preferred for easier querying in the current schema.

### 2. Update `DiagnosticManifest` Struct
Add `parent_id: Option<String>` to `src/processor/diagnostic/diagnostic_manifest.rs`.
- **Rationale**: The manifest is the source of truth during the processing lifecycle. Storing the `parent_id` here allows it to be passed down when recursive processing of `included_diagnostics` occurs.
- **Implementation**: The `parent_id` will be set during the `processor` lifecycle when a manifest with `included_diagnostics` is encountered.

### 3. Orchestration Mapping
Map `Product` types to orchestration strings in the `DiagnosticMetadata` conversion logic.
- `Product::ECK` -> `elastic-cloud-kubernetes`
- `Product::KubernetesPlatform` -> `kubernetes-platform`
- (Future) ECE/Cloud Hosted logic will be added as these products are integrated.

## Risks / Trade-offs

- **[Risk]** Data model mismatch if downstream consumers expect strict schemas. → **Mitigation**: Add fields as `Option` to maintain backward compatibility for diagnostics that don't have this context.
- **[Risk]** ID Collisions. → **Mitigation**: Use the parent's `uuid` (which is a V4 UUID) as the `parent_id` to ensure uniqueness.
