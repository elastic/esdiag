### Requirement: Memory-efficient processing of large diagnostic payloads
The processing pipeline SHALL avoid loading large top-level JSON structures (arrays or objects) entirely into memory. Instead, it MUST process individual elements or key-value pairs in a streaming manner.

#### Scenario: Processing large node stats
- **WHEN** the system encounters a `node_stats` payload with many nodes
- **THEN** it processes each node's statistics sequentially or in small batches
- **AND** it does not allocate memory for all nodes simultaneously

### Requirement: Buffer reuse for repeated diagnostic elements
The system SHALL reuse allocated memory buffers when deserializing multiple elements of the same type in a sequence.

#### Scenario: Reusing buffers for index stats
- **WHEN** the system deserializes statistics for multiple indices in a loop
- **THEN** it reuses the same memory allocation for the temporary data structures of each index
- **AND** total allocations are significantly reduced compared to fresh allocations per index

### Requirement: Streaming of deep nested structures
The system SHALL support streaming deserialization of deeply nested structures, such as index mappings, where individual parent elements may contain thousands of child elements.

#### Scenario: Processing large mapping payload
- **WHEN** the system encounters a large mapping payload with many indices and thousands of fields per index
- **THEN** it streams the top-level indices
- **AND** it optionally streams the nested properties/fields to further reduce peak memory usage

### Requirement: Performance benchmarks for large diagnostic archives
The system SHALL meet specific performance targets for peak memory usage and execution time when processing large diagnostic archives.

#### Scenario: Benchmark peak memory usage
- **WHEN** processing the large diagnostic archive `tests/archives/diagnostic-b33109-2025-Sep-29--22_30_33.zip`
- **THEN** the peak memory usage (RSS) SHALL NOT exceed 500MB
- **AND** the peak memory usage SHOULD be significantly lower than the baseline established before this change

#### Scenario: Benchmark execution time
- **WHEN** processing the large diagnostic archive `tests/archives/diagnostic-b33109-2025-Sep-29--22_30_33.zip`
- **THEN** the total execution time SHALL NOT increase by more than 10% compared to the baseline
- **AND** ideally, execution time SHOULD remain stable or decrease due to reduced allocation overhead
