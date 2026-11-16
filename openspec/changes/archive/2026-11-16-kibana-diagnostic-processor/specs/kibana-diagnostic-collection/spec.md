## ADDED Requirements

### Requirement: Current Kibana API compatibility
The system SHALL collect Kibana APIs through routes that the target Kibana version serves, including internal routes, and SHALL distinguish absent optional data from collection failures.

#### Scenario: Internal route collection
- **WHEN** the system collects a Kibana API, including an internal route such as `/internal/security/me` on Kibana 9
- **THEN** the request includes the `X-Elastic-Internal-Origin: Kibana` header so Kibana does not reject it as unavailable with the current configuration

#### Scenario: Removed Kibana APIs
- **WHEN** the target Kibana version no longer serves a source's legacy route, such as `/api/actions`, `/api/alerts/_find`, the Fleet packages `experimental` parameter, or `/api/fleet/agents/current_upgrades`
- **THEN** the system requests the replacement route for that version: `/api/actions/connectors`, `/api/alerting/rules/_find`, `withPackagePoliciesCount=true`, or `/api/fleet/agents/action_status`

#### Scenario: Absent exception list
- **WHEN** an exception list items request returns HTTP 404 because the exception list does not exist
- **THEN** the system treats the source as absent, writes no file, and does not record a collection failure
