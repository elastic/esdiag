## MODIFIED Requirements

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

## REMOVED Requirements

### Requirement: Custom Build Script Tarball Generation
**Reason**: Replaced by standard `rust-embed` library which provides built-in compression and better development ergonomics (like `debug-embed`).
**Migration**: Remove the custom `tar`/`flate2` logic in `build.rs` and the `tar` extraction logic in `src/assets.rs` (or equivalent).