## Why
Currently, ESDiag does not collect information about Elasticsearch snapshot repositories and snapshots. This information is crucial for diagnosing backup/restore issues and understanding the state of data durability in a cluster.

## What Changes
- Add support for collecting Snapshot Repository information via the `_snapshot` API.
- Add support for collecting Snapshot details via the `/_snapshot/*/*?verbose=false` API endpoint.
- Integrate these new data sources into the `ElasticsearchDiagnostic` processor to ensure they are included in diagnostic reports.

## Capabilities

### New Capabilities
- `snapshot-api`: Collection and processing of Elasticsearch snapshot repositories and snapshots.

### Modified Capabilities

## Impact
- New modules in `src/processor/elasticsearch/` for snapshot data structures and processing logic.
- Updates to `ElasticsearchDiagnostic` in `src/processor/elasticsearch/mod.rs` to register and execute the new snapshot processor.
- Addition of snapshot and repository data to the exported diagnostic documents.
