# Indexed Data Model

## Purpose

Defines the schema ESDiag owns for the documents it indexes: the ECS-inspired,
source-API-aligned field shapes, the provenance envelope layered on them, the output
data-stream naming contract, and how renamed provenance fields stay queryable across
index generations.

## Requirements

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
Kibana dashboards — and is NOT derived from a single source of truth. The two code-owned
layers SHALL be reconciled by test: every stream name a processor emits MUST have a
matching index template, and every ESDiag-owned index template MUST match a stream name
some processor emits. Dashboards SHALL be authored against the convention; because they
ship as Kibana saved objects in the repository, they SHALL also be verified by test —
every shipped data view MUST name a stream following the convention, including the
`-esdiag` suffix, so that a data view cannot match indices ESDiag does not own.

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

#### Scenario: Shipped data view names an ESDiag-owned stream
- **WHEN** a Kibana data view is shipped in the ESDiag saved objects
- **THEN** its index pattern MUST name a stream following the convention and MUST pin the `-esdiag` suffix rather than matching on a bare `{class}-{subtype}` prefix

### Requirement: Field-alias bridge for renamed provenance fields
The system SHALL bridge the provenance-field rename with Elasticsearch field aliases
rather than reindexing, because indexed data is semi-owned: it controls the `esdiag@*`
templates going forward but cannot rewrite historical indices. `diagnostic.application`
SHALL replace `diagnostic.product`, and `diagnostic.platform` SHALL replace
`diagnostic.orchestration`, with both names of each pair resolving to the same underlying
field via aliases in **both directions**, so a dashboard querying either name works
across an index pattern spanning old and new indices.

Bidirectionality is a property of the **index pattern**, not of a single index: a field
alias must target a concrete field, so each index SHALL carry exactly one of the pair as
an alias, in the direction its own stored name dictates. The templates SHALL give every
newly created index the legacy name as an alias to the current one, and the system SHALL
install the mirrored alias — the current name pointing at the stored legacy field — on
indices created before the rename. Installing the mirrored alias is a mapping addition
and SHALL NOT rewrite any stored document. It SHALL be idempotent, skipping indices that
already resolve both names, and SHALL be best-effort: an index that cannot accept a
mapping update MUST be reported without failing asset installation.

The provenance aliases SHALL be transitional and removable once dashboards are updated
and old indices age out of retention. Because dashboards ship as saved objects, the
system SHALL verify by test that no alias is removed while a shipped saved object still
queries the legacy name it serves.

#### Scenario: Old dashboard queries the legacy field on a new index
- **WHEN** a dashboard queries `diagnostic.product` against an index written with the new schema
- **THEN** the query MUST resolve via the alias to the same field as `diagnostic.application`

#### Scenario: New dashboard queries the new field on an old index
- **WHEN** a dashboard queries `diagnostic.application` against a historical index written with `diagnostic.product`
- **THEN** the query MUST resolve via the mirrored alias installed on that index to the stored `product` field

#### Scenario: Mirrored alias installation is idempotent
- **WHEN** the mirrored aliases are installed over indices that already resolve both provenance names
- **THEN** no mapping update MUST be issued for them

#### Scenario: An index that cannot be updated does not fail setup
- **WHEN** an index rejects the mirrored alias because it is closed, frozen, or write-blocked
- **THEN** the failure MUST be reported and asset installation MUST still succeed

#### Scenario: Old dashboard queries the legacy platform field on a new index
- **WHEN** the templates replace `diagnostic.orchestration` with `diagnostic.platform`
- **THEN** the query MUST resolve via the alias to the same field as `diagnostic.platform`

#### Scenario: Aliases are removable
- **WHEN** dashboards have been migrated and historical indices carrying legacy provenance fields have aged out of retention
- **THEN** the provenance aliases MUST be removable without breaking remaining dashboards
