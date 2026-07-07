## ADDED Requirements

### Requirement: Provenance envelope is ECS-inspired and source-API-aligned
The indexed diagnostic schema (the `esdiag@*` component and index templates) SHALL be
ECS-*inspired* but not ECS-compliant: field names and shapes SHALL stay aligned to the
source API's output so a user who knows the raw Elasticsearch/Logstash API recognizes
the fields. New fields SHALL mirror the source API's shape first and borrow ECS
conventions only where they do not obscure the source. The ESDiag-specific provenance
envelope (`diagnostic.*`, `cluster.*`) SHALL be layered on top of that source-shaped
payload.

#### Scenario: New field mirrors the source API
- **WHEN** a new indexed field is added for data exposed by a source API
- **THEN** its name and shape MUST follow the source API's output rather than being remapped to a strict ECS name

#### Scenario: Provenance envelope is layered on the payload
- **WHEN** a processed document is emitted
- **THEN** it MUST carry the `diagnostic.*` / `cluster.*` provenance envelope on top of the source-shaped payload

### Requirement: Output data-stream naming is a verified contract
Processed documents SHALL be written to data streams named by the single convention
`{class}-{subtype}[.sub]-esdiag`, where class ∈ `metrics | settings | logs | health`.
This name is a contract spanning three layers — processor code, index templates, and
Kibana dashboards — and is NOT derived from a single source of truth. The two
ESDiag-owned layers SHALL be reconciled by test: every stream name a processor emits
MUST have a matching index template, and every ESDiag-owned index template MUST match a
stream name some processor emits. Dashboards SHALL be authored against the convention and
maintained by review discipline.

#### Scenario: Emitted stream name has a matching template
- **WHEN** the processor emits documents to a stream named `metrics-<subtype>-esdiag`
- **THEN** a matching ESDiag-owned index template MUST exist for that stream pattern

#### Scenario: Test catches processor/template drift
- **GIVEN** the consistency test runs over the ESDiag-owned processor and index-template layers
- **WHEN** a processor emits a stream name with no matching index template, or a template matches no emitted stream
- **THEN** the test MUST fail

#### Scenario: Name follows the class convention
- **WHEN** a new output stream is introduced
- **THEN** its name MUST use a `class` drawn from `metrics | settings | logs | health` and end with the `-esdiag` suffix

### Requirement: Field-alias bridge for renamed provenance fields
The system SHALL bridge the provenance-field rename with Elasticsearch field aliases
rather than reindexing, because indexed data is semi-owned: it controls the `esdiag@*`
templates going forward but cannot rewrite historical indices. `diagnostic.application`
SHALL replace `diagnostic.product`, with both names resolving to the same underlying
field via aliases in **both directions**, so a dashboard querying either name works on
both old and new indices. `diagnostic.platform` SHALL replace
`diagnostic.orchestration`, with `diagnostic.orchestration` resolving to the new platform
field through a transitional alias. The provenance aliases SHALL be transitional and
removable once dashboards are updated and old indices age out of retention.

#### Scenario: Old dashboard queries the legacy field on a new index
- **WHEN** a dashboard queries `diagnostic.product` against an index written with the new schema
- **THEN** the query MUST resolve via the alias to the same field as `diagnostic.application`

#### Scenario: New dashboard queries the new field on an old index
- **WHEN** a dashboard queries `diagnostic.application` against a historical index written with `diagnostic.product`
- **THEN** the query MUST resolve via the alias to the stored `product` field

#### Scenario: Old dashboard queries the legacy platform field on a new index
- **WHEN** the templates replace `diagnostic.orchestration` with `diagnostic.platform`
- **THEN** the query MUST resolve via the alias to the same field as `diagnostic.platform`

#### Scenario: Aliases are removable
- **WHEN** dashboards have been migrated and historical indices carrying legacy provenance fields have aged out of retention
- **THEN** the provenance aliases MUST be removable without breaking remaining dashboards
