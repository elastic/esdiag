# Asset Compression

## Purpose
The purpose of this capability is to provide a storage-efficient mechanism for embedding and accessing static assets (such as Elasticsearch mappings and Kibana dashboards) within the `esdiag` binary. By using compression, the project maintains a single-binary distribution while minimizing binary bloat as the number of assets grows.

## Requirements

### Requirement: Asset Compression
The system SHALL bundle assets into a compressed archive during the build process to minimize binary size, utilizing the `rust-embed` crate with its `compression` feature enabled.

#### Scenario: Build-time asset compression
- **WHEN** the project is compiled
- **THEN** all assets in the assets directory are compressed and embedded via `rust-embed` macros.

### Requirement: Embedded Asset Access
The system SHALL embed the compressed asset archive into the binary and provide a mechanism to access individual assets at runtime directly from memory without writing to disk.

#### Scenario: Runtime asset extraction
- **WHEN** the `setup` command or internal asset lookup is executed
- **THEN** the system accesses the necessary assets directly from the `rust-embed` generated structs.

### Requirement: Storage Efficiency
The compressed asset bundle SHALL result in a smaller or comparable binary footprint compared to the legacy custom tarball generation.

#### Scenario: Binary size reduction
- **WHEN** comparing a binary built with the legacy custom tarball script versus one built with `rust-embed` compression
- **THEN** the binary size remains efficient and optimized.
