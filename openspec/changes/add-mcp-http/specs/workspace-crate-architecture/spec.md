## ADDED Requirements

### Requirement: MCP HTTP Transport Integration Stays In The GUI Crate
Code that integrates the MCP Streamable HTTP transport SHALL live only in `esdiag-gui`, and `esdiag-gui` SHALL enable that transport only through its own `mcp` feature. The root package SHALL forward its `mcp` feature to `esdiag-gui` as an optional-dependency feature (`esdiag-gui?/mcp`), so the transport is compiled only when the root's `mcp` and `server` features are both enabled; root `mcp` alone SHALL NOT pull in `esdiag-gui`. `esdiag-core` SHALL contain the transport-independent MCP tool catalog, SHALL NOT reference an HTTP transport, and SHALL NOT enable one through its own features. Because Cargo unifies features per package, a build that includes `esdiag-gui` with `mcp` MAY compile the shared MCP SDK with its HTTP transport.

#### Scenario: Server and MCP build
- **GIVEN** the root package
- **WHEN** a maintainer builds it with default features disabled and the `server` and `mcp` features enabled
- **THEN** `esdiag-gui` is compiled with the MCP HTTP transport
- **AND** Tauri is not compiled

#### Scenario: MCP without server compiles no GUI
- **GIVEN** the root package
- **WHEN** a maintainer builds it with default features disabled and only the `mcp` feature enabled
- **THEN** `esdiag-gui` is not compiled
- **AND** the MCP SDK's Streamable HTTP server transport is not compiled

#### Scenario: Core alone has no HTTP transport
- **GIVEN** the `esdiag-core` package alone, with its `mcp` feature enabled
- **WHEN** a maintainer lists its normal dependency features
- **THEN** the MCP SDK is present without its Streamable HTTP server transport
- **AND** `esdiag-gui` is not present
