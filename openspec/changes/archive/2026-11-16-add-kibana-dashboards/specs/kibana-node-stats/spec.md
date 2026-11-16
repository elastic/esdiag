## ADDED Requirements

### Requirement: Node Stats Concurrent Connections
The system SHALL preserve `concurrent_connections` from `kibana_stats.json` as `node.concurrent_connections` in `metrics-kibana.node-esdiag` documents.

#### Scenario: Concurrent connections preserved
- **GIVEN** a `kibana_stats.json` response containing `"concurrent_connections": 12`
- **WHEN** the response is processed
- **THEN** the exported node document contains `node.concurrent_connections` equal to `12`

### Requirement: Node Stats Dashboard Field Mappings
The `metrics-kibana.node-esdiag` index template SHALL explicitly map every `node.process`, `node.os`, `node.response_times`, `node.requests`, `node.concurrent_connections`, and `node.elasticsearch_client` field that the Kibana Host Health dashboard uses. Byte and count values SHALL be `long`, and load, event loop, utilization, and response time values SHALL be `double`.

#### Scenario: Event loop delay percentiles are aggregatable
- **GIVEN** a node document containing `node.process.event_loop_delay_histogram.percentiles.99`
- **WHEN** the document is indexed with the installed templates
- **THEN** the field is mapped as `double` and can be aggregated, even though the shared `skip_histograms` dynamic template disables other `*histogram` objects

#### Scenario: Container fields resolvable without container data
- **GIVEN** no indexed node document contains `node.os.cpu` or `node.os.cgroup_memory`
- **WHEN** the Kibana node stats data view loads its fields
- **THEN** the cgroup fields used by the Kibana Host Health dashboard are present in the field list

#### Scenario: Fractional load after integer load
- **GIVEN** a first document holds an integer `node.os.load.1m`
- **WHEN** a later document holds a fractional `node.os.load.1m`
- **THEN** the fractional value is indexed without truncation
