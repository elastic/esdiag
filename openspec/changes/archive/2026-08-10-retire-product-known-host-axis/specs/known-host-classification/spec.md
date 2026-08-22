## ADDED Requirements

### Requirement: Orthogonal Known-Host Classification
The system SHALL classify a saved host independently by its target `Application`, its
endpoint state (concrete or template-backed), and its route metadata. The system MUST
NOT encode a deployment platform, Cloud admin route, template state, or uncertainty as
an `Application`.

#### Scenario: Direct Elasticsearch host
- **GIVEN** a saved host targets a concrete Elasticsearch endpoint
- **WHEN** the system classifies the host
- **THEN** its application is `Elasticsearch`
- **AND** its route is direct

#### Scenario: Elasticsearch through a Cloud admin route
- **GIVEN** a concrete saved host reaches Elasticsearch APIs through a recognized Elastic Cloud admin proxy URL
- **WHEN** the system classifies the host
- **THEN** its application is `Elasticsearch`
- **AND** its route identifies the Cloud admin transport
- **AND** neither `ElasticCloudHosted` nor the Cloud admin route is stored as its application

#### Scenario: Dynamic template has no application before resolution
- **GIVEN** a template-backed host whose URL template accepts a product parameter
- **WHEN** the template is stored before a reference selects a product
- **THEN** the host remains unresolved with no target application
- **AND** the system does not assign an `Unknown` application

### Requirement: Resolved Host Runtime Boundary
The system SHALL construct clients, receivers, and collectors only from a resolved host
that has a concrete URL and one API-collectable application: `Elasticsearch`, `Kibana`,
or `Logstash`. Resolution MUST validate the application, rendered URL, route metadata,
and role compatibility as one operation.

#### Scenario: Materialize a dynamic template reference
- **GIVEN** a template-backed host with placeholders for identifier and product
- **WHEN** a reference supplies an identifier and selects `kibana`
- **THEN** the system renders a concrete URL
- **AND** returns a resolved host whose application is `Kibana`
- **AND** validates the resolved host before runtime dispatch

#### Scenario: Reject unresolved template at runtime
- **GIVEN** a template-backed saved host has not been materialized with an identifier and application
- **WHEN** a caller attempts to construct a runtime receiver from it
- **THEN** the system rejects the request with guidance for resolving the template

#### Scenario: Reject ambiguous concrete record at runtime
- **GIVEN** a legacy concrete host has no application
- **AND** its endpoint does not identify an application unambiguously
- **WHEN** a caller attempts to use it at runtime
- **THEN** the system rejects the request with guidance to assign an application
- **AND** does not silently default the host to Elasticsearch

### Requirement: Legacy Saved-Host Read Compatibility
The system SHALL continue to deserialize legacy saved-host records that encode
application values, `Unknown`, `None`, or platform values in `app`. It SHALL normalize
actual application values to `Application` and SHALL treat uncertainty or platform-only
values as an absent application.

#### Scenario: Read legacy application value
- **GIVEN** a legacy host record contains `app: Elasticsearch`
- **WHEN** the system reads the record
- **THEN** the target application is normalized to `Elasticsearch`

#### Scenario: Read legacy unknown placeholder
- **GIVEN** a legacy template-backed host contains `app: Unknown`
- **WHEN** the system reads the record
- **THEN** the record remains readable
- **AND** its application is absent until template resolution

#### Scenario: Read legacy platform value
- **GIVEN** a legacy host record contains a recognized platform value in `app`
- **WHEN** the system reads the record
- **THEN** the record remains readable
- **AND** the platform value is not converted into an application

### Requirement: Normalized Saved-Host Writes
The system SHALL serialize only actual `Application` values in the saved-host `app`
field. It SHALL omit `app` for an unresolved template and MUST NOT write `Unknown`,
`None`, a platform, or route metadata into that field.

#### Scenario: Write concrete classified host
- **GIVEN** a resolved concrete host targets Logstash
- **WHEN** the system persists the host
- **THEN** the record contains the Logstash application value in `app`

#### Scenario: Write unresolved template host
- **GIVEN** a valid dynamic template-backed host has no application until materialization
- **WHEN** the system persists the template
- **THEN** the record omits `app`
- **AND** the record contains no placeholder application value

### Requirement: Legacy Manifest Product Isolation
The system SHALL preserve the legacy diagnostic manifest field named `product` and all
historically accepted wire values after retiring the general domain `Product` type. The
wire value SHALL be interpreted only at the manifest boundary and SHALL NOT be reused
as known-host or runtime dispatch classification.

#### Scenario: Read historical manifest product
- **GIVEN** a historical bundle manifest contains a legacy platform or application value in `product`
- **WHEN** the system loads the bundle
- **THEN** the manifest deserializes under the permanent compatibility contract
- **AND** the value is converted to platform and optional application classifications for internal use

#### Scenario: Write compatible manifest product field
- **GIVEN** ESDiag writes a diagnostic manifest
- **WHEN** the manifest is serialized
- **THEN** the existing `product` field name and wire shape remain unchanged
