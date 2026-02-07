## Context

Currently, `esdiag` collects index statistics via the `IndicesStats` processor, but it does not include information about the index mappings. Analyzing mappings is crucial for understanding the data structure and field type distribution, which helps in identifying mapping explosions or inefficient field usage.

## Goals / Non-Goals

**Goals:**
- Implement a `MappingStats` processor to parse `mapping.json`.
- Summarize field types (counts per type) for each index.
- Extract `dynamic` and `_meta` from mappings.
- Enrich `IndexStatsDocument` with this summarized mapping data.
- Ensure the summarization logic is efficient and handles nested fields/multi-fields.

**Non-Goals:**
- Importing the full mapping JSON into Elasticsearch.
- Supporting complex mapping features like `runtime` fields or `transform` in the summary.

## Decisions

- **New `MappingStats` Module**: Create `src/processor/elasticsearch/mapping_stats/` with `data.rs` and `processor.rs`.
- **`DataSource` Implementation**: `MappingStats` will implement `DataSource`, reading from `mapping.json`.
- **Struct-based Deserialization**: To handle large mapping files efficiently, we will define a set of structs for deserialization instead of using `serde_json::Value`. The deserializer will be configured with `#[serde(flatten)]` or `#[serde(other)]` where appropriate, or simply by not defining fields we don't need (permissive parsing).
- **Mappings Data Model**:
  ```rust
  struct MappingFile(HashMap<String, IndexMapping>);
struct IndexMapping { mappings: Mappings }
struct Mappings { 
    dynamic: Option<serde_json::Value>, 
    date_detection: Option<bool>,
    numeric_detection: Option<bool>,
    dynamic_date_formats: Option<Vec<String>>,
    dynamic_templates: Option<Vec<serde_json::Value>>,
    _data_stream_timestamp: Option<DataStreamTimestamp>,
    _source: Option<SourceMode>,
    _meta: Option<serde_json::Value>, 
    properties: Option<HashMap<String, FieldDefinition>> 
}
struct SourceMode {
    mode: Option<String>,
}
...
pub struct MappingSummary {
    pub dynamic: Option<serde_json::Value>,
    pub date_detection: Option<bool>,
    pub numeric_detection: Option<bool>,
    pub dynamic_date_formats: Option<Vec<String>>,
    pub dynamic_templates: Option<u32>,
    pub _data_stream_timestamp: Option<DataStreamTimestamp>,
    pub _source: Option<SourceMode>,
    pub _meta: Option<serde_json::Value>,
    pub field: FieldSummary,
}
pub struct FieldSummary {
    pub fields: HashMap<String, u64>, // Renamed from count, includes "total"
}

struct DataStreamTimestamp {
    enabled: bool,
}
#[derive(Deserialize)]
...
pub struct MappingSummary {
    pub dynamic: Option<serde_json::Value>,
    pub dynamic_date_formats: Option<Vec<String>>,
    pub dynamic_templates: Option<u32>,
    pub _data_stream_timestamp: Option<DataStreamTimestamp>,
    pub _meta: Option<serde_json::Value>,
    pub field: FieldSummary,
}
pub struct FieldSummary {
    pub count: HashMap<String, u64>, // Includes "total" and type counts
}

  #[derive(Deserialize)]
  #[serde(rename_all = "snake_case")]
  struct FieldDefinition {
      #[serde(rename = "type")]
      field_type: Option<String>,
      properties: Option<HashMap<String, FieldDefinition>>,
      fields: Option<HashMap<String, FieldDefinition>>,
      #[serde(flatten)]
      _extra: HashMap<String, serde_json::Value>, // Permissive parsing
  }
  ```
- **`Lookups` Integration**: Add `mapping_stats: Lookup<MappingSummary>` to the `Lookups` struct in `src/processor/elasticsearch/mod.rs`.
- **Recursive Summarization**: Use a recursive function to traverse the `FieldDefinition` tree and increment counters for each field type.
  - Multi-fields (`fields` property) will be counted as additional fields.
  - Nested objects will be traversed via their `properties`.
- **Enrichment in `IndicesStats`**: Update `EnrichedIndexStatsWithSettings` to include a `mappings: Option<MappingSummary>` field. The `IndicesStats::documents_export` method will perform the lookup by index name.

## Risks / Trade-offs

- **[Risk] Large Mapping Files** → Large `mapping.json` files could consume significant memory during parsing.
  - *Mitigation*: The summary itself is compact. We will parse and summarize, then drop the full mapping representation.
- **[Risk] Complex Mapping Structures** → Deeply nested or unusual mappings might cause recursion issues or inaccurate counts.
  - *Mitigation*: Use a depth limit or iterative approach if necessary, though typical mappings should be fine with recursion. Multi-fields will be treated as separate fields to accurately reflect the indexing overhead.
- **[Trade-off] Multi-fields Counting** → We will count each multi-field as a separate instance of its type. This accurately reflects the number of fields Elasticsearch has to index.
