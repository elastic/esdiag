## ADDED Requirements

### Requirement: Record orchestration platform
The system SHALL identify and record the orchestration platform that generated the diagnostic bundle.

#### Scenario: Identify ECK
- **WHEN** a diagnostic bundle is processed from Elastic Cloud Kubernetes
- **THEN** the `diagnostic.orchestration` field MUST be set to `elastic-cloud-kubernetes`

#### Scenario: Identify ECE
- **WHEN** a diagnostic bundle is processed from Elastic Cloud Enterprise
- **THEN** the `diagnostic.orchestration` field MUST be set to `elastic-cloud-enterprise`

#### Scenario: Identify Elastic Cloud Hosted
- **WHEN** a diagnostic bundle is processed from Elastic Cloud Hosted
- **THEN** the `diagnostic.orchestration` field MUST be set to `elastic-cloud-hosted`

#### Scenario: Identify Kubernetes Platform
- **WHEN** a diagnostic bundle is processed from a generic Kubernetes Platform
- **THEN** the `diagnostic.orchestration` field MUST be set to `kubernetes-platform`

### Requirement: Record parent diagnostic relationship
The system SHALL record the relationship between a parent diagnostic bundle and its included children diagnostics.

#### Scenario: Set parent_id for included diagnostics
- **WHEN** a diagnostic bundle (parent) contains `included_diagnostics`
- **THEN** each included diagnostic (child) SHALL have its `diagnostic.parent_id` set to the parent's UUID
