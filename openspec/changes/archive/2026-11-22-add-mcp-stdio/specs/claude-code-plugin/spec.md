## ADDED Requirements

### Requirement: Plugin Registers The Local MCP Server
The Claude Code and Codex plugin packages SHALL register one MCP server named `esdiag` that the host starts as `esdiag mcp` from the user's `PATH`. The registration SHALL contain no URLs, credentials, environment values, or connection settings, so it uses ESDiag's canonical application configuration. Skill guidance SHALL prefer the ESDiag MCP tools when the host has them, and SHALL use the equivalent CLI commands otherwise.

#### Scenario: Plugin installation enables the tools
- **GIVEN** `esdiag` built with `mcp` is on the user's `PATH`
- **WHEN** the user installs the ESDiag plugin into Claude Code or Codex
- **THEN** the host lists the ESDiag MCP tools
- **AND** the user configured no plugin-specific connection settings

#### Scenario: Registration carries no configuration
- **WHEN** a maintainer validates the plugin packages
- **THEN** each MCP server registration contains only the `esdiag` command and the `mcp` argument

#### Scenario: Host without MCP tools
- **GIVEN** a host session where the ESDiag MCP tools are unavailable
- **WHEN** the user requests a diagnostic review
- **THEN** the skill uses the native CLI commands as before
