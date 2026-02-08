## Why

The current implementation uses the `include_dir!` macro to embed all Elasticsearch and Kibana assets. This approach is inefficient as it creates inlined byte arrays from text, leading to binary bloat. As the project grows to include more assets (Kibana assets, explicit mappings, knowledge base articles), the binary size will become unmanageable.

## What Changes

- Replace `include_dir!` with a more storage-efficient bundling mechanism.
- Bundle the assets directory into a compressed archive (e.g., `.zip` or `.tar.gz`) during the build process.
- Use `include_bytes!` to embed the compressed archive into the binary.
- Implement a runtime decompression/extraction mechanism to access assets when needed (specifically during the `setup` phase).

## Capabilities

### New Capabilities
- `asset-compression`: Mechanisms for bundling, compressing, and extracting embedded assets at build and runtime.

### Modified Capabilities
<!-- No requirement changes to existing specs, as this is an implementation/optimization change. -->

## Impact

- **Build System**: Requires a build step to compress assets before compilation.
- **Binary Size**: Significant reduction in binary size due to text compression.
- **Runtime**: Minor overhead during the `setup` command for extraction; however, `setup` is a one-time operation.
- **Codebase**: Changes to `src/setup.rs` and related asset-handling logic.
- **Dependencies**: Potential new dependency for archive handling (e.g., `zip` or `flate2`).
