## ADDED Requirements

### Requirement: Collect Stage Product Scope
The `Collect` stage SHALL acquire live diagnostics only for the **API-collectable**
products — Elasticsearch, Kibana, and Logstash. The system SHALL NOT API-collect
Elastic Agent or any platform (ECE, ECK, KubernetesPlatform); those diagnostics are
product-provided and enter the pipeline exclusively via `Load`. When a `Collect`
request targets a product outside the API-collectable set, the system SHALL refuse it
as **out-of-scope by design** — distinct from a not-yet-implemented error — and SHALL
direct the caller to acquire that diagnostic via `Load` (CLI `read`, Web UI `Upload`).

#### Scenario: Collect an API-collectable product
- **WHEN** a user runs `esdiag collect` against an Elasticsearch, Kibana, or Logstash host
- **THEN** the system constructs a `Collect` receiver and pulls the resolved live APIs

#### Scenario: Collect refuses Elastic Agent
- **WHEN** a user requests a `Collect` against an Elastic Agent target
- **THEN** the system MUST refuse the request as out-of-scope by design
- **AND** the message MUST state that Elastic Agent provides its own diagnostic bundle to be acquired via `Load`, not API-collected

#### Scenario: Collect refuses a platform target
- **WHEN** a user requests a `Collect` against an ECE, ECK, or KubernetesPlatform target
- **THEN** the system MUST refuse the request as out-of-scope by design
- **AND** the message MUST direct the caller to `Load` the platform-generated bundle instead

#### Scenario: By-design refusal is not a not-yet-implemented error
- **WHEN** a `Collect` request is refused because its target is not an API-collectable product
- **THEN** the refusal MUST be reported as a deliberate scope boundary
- **AND** it MUST NOT be reported as unimplemented or work-in-progress collection

### Requirement: Collection Definition Covers API-Collectable Products Only
The collection definition registry (`assets/<product>/sources.yml`) SHALL describe API
sources only for the API-collectable products (Elasticsearch, Kibana, Logstash). The
system SHALL NOT define or resolve API sources for Elastic Agent or any platform.

#### Scenario: No API source set for a product-provided product
- **WHEN** the collect list is resolved for a run
- **THEN** the system resolves API sources only from the Elasticsearch, Kibana, or Logstash collection definitions
- **AND** no API source set exists for Elastic Agent or any platform
