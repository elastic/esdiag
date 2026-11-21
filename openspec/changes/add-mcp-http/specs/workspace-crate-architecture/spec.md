## ADDED Requirements

### Requirement: MCP HTTP Transport Stays In The GUI Crate
The MCP Streamable HTTP transport SHALL be compiled only in `esdiag-gui`, and only when both the `mcp` and `server` features are enabled. `esdiag-core` SHALL contain the transport-independent MCP tool catalog and SHALL NOT depend on an HTTP transport.

#### Scenario: Server and MCP build
- **GIVEN** the root package
- **WHEN** a maintainer builds it with default features disabled and the `server` and `mcp` features enabled
- **THEN** `esdiag-gui` is compiled with the MCP HTTP transport
- **AND** Tauri is not compiled

#### Scenario: Core has no HTTP transport
- **GIVEN** a build with default features
- **WHEN** a maintainer lists the normal dependency features of `esdiag-core`
- **THEN** the MCP SDK is present without its Streamable HTTP server transport
