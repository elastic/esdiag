## Why

The ESDiag application has a severe memory fragmentation and parsing latency issue when streaming massive JSON diagnostics (1GB+ files). This occurs because generic objects in the JSON payload are parsed using `serde_json::Value`, which forces the global allocator to build millions of tiny, dynamic `BTreeMap` and `String` allocations. While `mimalloc` fixed the OOM crashes caused by fragmentation, `Value` remains extremely slow to parse and wastes massive amounts of memory holding the intermediate DOM trees.

Replacing generic `Value` types with `Box<RawValue>` across the codebase will completely eliminate these intermediate `BTreeMap` allocations, dramatically improving CPU throughput and keeping memory usage consistently low by storing the raw, unparsed JSON string bytes directly.

## What Changes

- Enable the `raw_value` feature on the `serde_json` dependency.
- Replace all occurrences of `serde_json::Value` with `Box<serde_json::value::RawValue>` in core diagnostic models, primarily targeting `elasticsearch` processors like `nodes_stats` and `indices_stats`.
- Ensure no existing legacy processors currently rely on mutating these flexible objects via `json_patch` before applying this optimization. If mutation is required, explicit Rust structs must be modeled out instead of falling back to DOM manipulation.

## Capabilities

### New Capabilities
- `raw-value`: Raw string serialization

### Modified Capabilities

## Impact

- **Affected code:** The core data models across all Elastic product diagnostic processors (`elasticsearch`, `logstash`, `kibana`, etc.) where flexible/unknown JSON schemas are captured.
- **Dependencies:** `serde_json` configuration.
- **Performance:** CPU time spent parsing diagnostics will significantly drop; Resident Set Size (RSS) will remain extremely flat and low compared to the legacy implementation.