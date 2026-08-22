## ADDED Requirements

### Requirement: Web Host Classification Parity
The Web host manager SHALL use the same application, endpoint-state, route, template
resolution, and role validation rules as the CLI. The Web form MUST offer only actual
Stack applications as application values and MUST NOT expose a platform, Cloud admin
route, or `Unknown` as an application choice.

#### Scenario: Create concrete Cloud admin host
- **GIVEN** a user enters a recognized Cloud admin proxy URL for Elasticsearch
- **WHEN** the user saves the host form
- **THEN** the backend classifies the target application as Elasticsearch
- **AND** preserves Cloud admin as route metadata
- **AND** returns the normalized host record to the UI

#### Scenario: Platform is absent from application choices
- **WHEN** the Web host manager renders the application selector
- **THEN** ECK, ECE, Elastic Cloud Hosted, Kubernetes Platform, and Unknown are not application choices

#### Scenario: Backend rejects forged platform application
- **GIVEN** a request bypasses the rendered application selector
- **WHEN** it submits a platform value as the host application
- **THEN** backend validation rejects the request
- **AND** persisted host storage remains unchanged

### Requirement: Web Template Resolution State
The Web host manager SHALL distinguish an unresolved template-backed record from a
concrete classified host. It SHALL render an unresolved template without a fake
application and SHALL require successful materialization before any connection or
authentication test that needs a concrete endpoint.

#### Scenario: Display unresolved dynamic template
- **GIVEN** a saved dynamic template has no application before materialization
- **WHEN** the Web host manager renders the record
- **THEN** the UI identifies the application as unresolved
- **AND** does not display `Unknown` or a platform as the application

#### Scenario: Test unresolved template
- **GIVEN** a dynamic template has not been supplied an identifier and application
- **WHEN** the user requests an authentication or connection test
- **THEN** the backend rejects the test with template-resolution guidance

#### Scenario: Materialize template from Web workflow
- **GIVEN** a Web workflow selects a saved dynamic template
- **WHEN** the user supplies an identifier and supported application
- **THEN** the backend renders and validates a resolved host
- **AND** only the resolved host is passed to runtime dispatch
