## ADDED Requirements

### Requirement: ESDiag Owns Its Collection Definitions
ESDiag's per-product `sources.yml` files SHALL be owned by ESDiag and shaped to its own model. The runtime MUST NOT mirror `support-diagnostics` verbatim or bind to upstream files at runtime; it SHALL read only ESDiag's embedded definitions (or a `--sources` override). `support-diagnostics` (`elastic-rest.yml` for API sources, `diags.yml` for OS-command sources) SHALL be treated only as a reconciliation *input*.

#### Scenario: Runtime binds to no upstream file
- **GIVEN** a diagnostic run resolving data sources
- **WHEN** the system loads its collection definitions
- **THEN** it reads only ESDiag's embedded (or `--sources`-overridden) `sources.yml`
- **AND** it does not read `elastic-rest.yml` or `diags.yml` at runtime

#### Scenario: ESDiag may deliberately diverge from upstream
- **GIVEN** ESDiag has added, removed, or corrected a source relative to upstream
- **WHEN** reconciliation runs
- **THEN** the recorded divergence SHALL be preserved and MUST NOT be silently reverted to the upstream definition

### Requirement: Field-Level Overlay Reconciliation
Reconciliation SHALL merge upstream REST API definitions into ESDiag's `sources.yml` as a field-level overlay, never a blind copy. It SHALL update upstream-owned fields (`versions`/request paths) while preserving ESDiag-only enrichments — at minimum `source_weight`, `processing_weight`, `streamable`, and `tags` (including platform/application tags). The merge MUST know which fields are ESDiag's so that hand-tuned enrichments survive. Until ESDiag has a command-source transport model, reconciliation SHALL verify the upstream `diags.yml` OS-command catalog path but MUST NOT merge command entries into the HTTP REST registry.

#### Scenario: Upstream path update preserves ESDiag enrichments
- **GIVEN** an existing source with hand-tuned `source_weight`, `processing_weight`, `streamable`, and `tags`
- **WHEN** reconciliation overlays a changed upstream `versions`/path for that source
- **THEN** the request path is updated
- **AND** the ESDiag-only enrichment fields are preserved unchanged

#### Scenario: New upstream source is added without clobbering others
- **GIVEN** upstream introduces a new REST API source
- **WHEN** reconciliation runs
- **THEN** the new source entry is added to ESDiag's `sources.yml`
- **AND** no existing entry's enrichment fields are overwritten

#### Scenario: OS-command catalog layout is verified but not overlaid yet
- **GIVEN** a support-diagnostics checkout with `src/main/resources/diags.yml`
- **WHEN** reconciliation runs
- **THEN** the script verifies the OS-command catalog path exists
- **AND** it does not merge OS-command entries into `sources.yml`

### Requirement: Semver Dialect Normalization at the Boundary
Reconciliation SHALL convert upstream version ranges from the Java/NPM semver dialect into native Rust `semver` form during the overlay, so that ESDiag's stored `sources.yml` is already in its own dialect. The impedance MUST be absorbed once, at reconciliation, so that the runtime can resolve versions with stock `semver::VersionReq` and carry no custom compatibility parser (see `version-dependent-sources`).

#### Scenario: Upstream range is normalized on ingest
- **GIVEN** an upstream version range expressed in the Java/NPM semver dialect
- **WHEN** reconciliation overlays that range into ESDiag's `sources.yml`
- **THEN** the stored range is in native Rust `semver` form
- **AND** it is resolvable by stock `semver::VersionReq` without a dialect shim

### Requirement: Recurring Reconciliation Cadence
Reconciliation SHALL be a required, recurring discipline performed on **every application release** (a new Elasticsearch / Kibana / Logstash version may add or change endpoints and their version-gating) **and every `support-diagnostics` release** (upstream may revise definitions or OS commands). Reconciliation MUST have a defined owner and cadence so that version-gating does not silently go stale.

#### Scenario: Application release triggers reconciliation
- **WHEN** a new Elasticsearch, Kibana, or Logstash version is released
- **THEN** reconciliation MUST be performed to capture any added or changed endpoints and their version-gating

#### Scenario: Upstream release triggers reconciliation
- **WHEN** a new `support-diagnostics` release is published
- **THEN** reconciliation MUST be performed to overlay any revised definitions or OS commands
