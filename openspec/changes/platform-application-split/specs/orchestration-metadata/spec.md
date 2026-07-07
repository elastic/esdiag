## MODIFIED Requirements

### Requirement: Record deployment platform
The system SHALL identify and record the **deployment platform** every diagnostic was
collected from as a typed, total value in the `diagnostic.platform` field (replacing
the former untyped `diagnostic.orchestration` string). The platform is one of
`SelfManaged | ElasticCloudHosted | ECE | ECK | KubernetesPlatform | Unknown` — every
diagnostic has exactly one, and there is no "no platform" case. The platform is
determined best-effort from indicators; when provenance cannot be established it MUST
be `Unknown`.

#### Scenario: Identify ECK
- **WHEN** a diagnostic bundle is processed from Elastic Cloud Kubernetes
- **THEN** the `diagnostic.platform` field MUST be set to `ECK`

#### Scenario: Identify ECE
- **WHEN** a diagnostic bundle is processed from Elastic Cloud Enterprise
- **THEN** the `diagnostic.platform` field MUST be set to `ECE`

#### Scenario: Identify Elastic Cloud Hosted
- **WHEN** a diagnostic bundle is processed from Elastic Cloud Hosted
- **THEN** the `diagnostic.platform` field MUST be set to `ElasticCloudHosted`

#### Scenario: Identify Kubernetes Platform
- **WHEN** a diagnostic bundle is processed from a generic Kubernetes Platform
- **THEN** the `diagnostic.platform` field MUST be set to `KubernetesPlatform`

#### Scenario: Identify self-managed from indicators
- **WHEN** a diagnostic bundle has no orchestration indicators but contains a `syscalls` folder
- **THEN** the `diagnostic.platform` field MUST be set to `SelfManaged`

#### Scenario: Indeterminate provenance
- **WHEN** a diagnostic bundle's platform cannot be established from any indicator (e.g. a legacy `support-diagnostics` bundle)
- **THEN** the `diagnostic.platform` field MUST be set to `Unknown`

## ADDED Requirements

### Requirement: Classify application component
The system SHALL record the Elastic Stack application a diagnostic pertains to in an
optional `diagnostic.application` field, drawn from the closed set `Elasticsearch |
Kibana | Logstash | Agent`. A diagnostic that carries only the platform's own data
(e.g. an ECE bundle, or the orchestration-level data of an ECK bundle) SHALL have no
application. The `application` axis SHALL NEVER hold a platform value.

#### Scenario: Application-level diagnostic
- **WHEN** an Elasticsearch diagnostic is processed
- **THEN** `diagnostic.application` MUST be `Elasticsearch` and `diagnostic.platform` MUST be present independently

#### Scenario: Platform-only diagnostic
- **WHEN** an ECE diagnostic (carrying no application data) is processed
- **THEN** `diagnostic.application` MUST be absent and `diagnostic.platform` MUST be `ECE`

#### Scenario: Display label falls back to platform
- **WHEN** a diagnostic has no `application`
- **THEN** its display label MUST be derived from `platform`; otherwise from `application`

### Requirement: Propagate platform to included diagnostics
The system SHALL set the `diagnostic.platform` on each included (child) diagnostic as
it is spawned, so a child inherits its parent's platform. Included diagnostics are
always application-layer and never introduce a different platform.

#### Scenario: Child inherits parent platform
- **WHEN** an ECK diagnostic includes an Elasticsearch child diagnostic
- **THEN** the child MUST have `diagnostic.platform` = `ECK` and `diagnostic.application` = `Elasticsearch`
