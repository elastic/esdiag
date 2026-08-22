## ADDED Requirements

### Requirement: Application-Based Host Role Validation
The system SHALL validate application-specific host roles against the resolved target
`Application`, independent of whether the endpoint uses a direct or Cloud admin route.
The `send` role MUST resolve to Elasticsearch and the `view` role MUST resolve to
Kibana.

#### Scenario: Cloud admin Elasticsearch target accepts send role
- **GIVEN** a concrete host reaches Elasticsearch through a Cloud admin route
- **AND** the host has the `send` role
- **WHEN** the system validates the host
- **THEN** validation succeeds because the resolved application is Elasticsearch
- **AND** the route is not treated as the application

#### Scenario: Direct Kibana target accepts view role
- **GIVEN** a concrete direct host resolves to Kibana
- **AND** the host has the `view` role
- **WHEN** the system validates the host
- **THEN** validation succeeds

#### Scenario: Platform value cannot satisfy role validation
- **GIVEN** a legacy host contains a platform value where an application is required
- **WHEN** the system validates a `send` or `view` role
- **THEN** the platform value does not satisfy the role constraint
- **AND** the system requires the host to resolve to a compatible application

### Requirement: Deferred Dynamic-Template Role Validation
The system SHALL validate role constraints immediately when a template fixes its
application. When a dynamic template defers application selection to materialization,
the system SHALL validate the saved role set against each selected application before
returning a resolved host.

#### Scenario: Materialized send template selects Elasticsearch
- **GIVEN** a dynamic template has the `send` role
- **WHEN** a reference materializes it with application `Elasticsearch`
- **THEN** role validation succeeds

#### Scenario: Materialized send template selects Kibana
- **GIVEN** a dynamic template has the `send` role
- **WHEN** a reference attempts to materialize it with application `Kibana`
- **THEN** resolution fails because `send` is valid only for Elasticsearch

#### Scenario: Fixed Kibana template rejects send role when saved
- **GIVEN** a template is fixed to application `Kibana`
- **WHEN** the user attempts to save it with the `send` role
- **THEN** validation fails without waiting for materialization
