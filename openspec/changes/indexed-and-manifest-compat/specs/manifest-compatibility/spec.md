## ADDED Requirements

### Requirement: Permanent backward read-compatibility for manifests
The system SHALL always be able to read the bundle manifest produced by
`support-diagnostics` and by every prior ESDiag version. A manifest is a read-only
interchange artifact received inside a bundle; the system SHALL NOT require, gate on, or
depend on a manifest version, and SHALL NOT rewrite a manifest in place. There is no
migration path — read tolerance carries all compatibility.

#### Scenario: Read a support-diagnostics manifest
- **WHEN** a bundle carrying a `support-diagnostics`-produced manifest is loaded
- **THEN** the manifest MUST deserialize successfully and the diagnostic MUST be processable

#### Scenario: Read an older-ESDiag manifest
- **WHEN** a bundle carrying a manifest written by any prior ESDiag version is loaded
- **THEN** the manifest MUST deserialize successfully without a version gate or migration step

### Requirement: Additive-only manifest evolution
The system SHALL evolve the manifest only by **adding** ESDiag-specific properties. It
SHALL NOT remove, rename, or repurpose any existing manifest field. New information MUST
be carried in new optional fields, and existing fields MUST NOT change meaning or shape.

#### Scenario: ESDiag-added property is present
- **WHEN** ESDiag writes a manifest containing its own added properties
- **THEN** those properties MUST be additional fields alongside the existing interchange fields, leaving every pre-existing field unchanged

#### Scenario: Existing field meaning is preserved
- **WHEN** a manifest field defined by `support-diagnostics` is read by ESDiag
- **THEN** ESDiag MUST interpret it with its original meaning and MUST NOT overload or repurpose it

### Requirement: Tolerant manifest deserialization
The system SHALL deserialize manifests tolerantly. Unknown fields MUST be ignored,
ESDiag-added fields MUST be optional and defaulted when absent, and missing values MUST
be inferred rather than causing a failure.

#### Scenario: Unknown field is ignored
- **WHEN** a manifest contains a field ESDiag does not recognize
- **THEN** ESDiag MUST ignore the field and continue deserialization without error

#### Scenario: ESDiag-added field is absent
- **WHEN** a manifest omits an ESDiag-added property (e.g. an older bundle predating it)
- **THEN** that property MUST default and the manifest MUST still deserialize successfully

### Requirement: Legacy Product resolved by inference, not migration
The system SHALL resolve the deployment platform of a manifest that predates the
`Platform`/`Application` split (one carrying the legacy single-axis `Product`, or no such
field at all) by **inference** at read time rather than by rewriting the manifest. The
platform MUST default to `Unknown` and be refined by indicators — a `syscalls` folder ⇒
`SelfManaged`, a manifest `runner` of `ece` ⇒ `ECE`.

#### Scenario: Legacy manifest with no orchestration indicators
- **WHEN** a legacy `support-diagnostics` manifest with no platform indicators is read
- **THEN** the platform MUST be inferred as `Unknown` and the stored manifest MUST NOT be modified

#### Scenario: Indicator refines an inferred platform
- **WHEN** a legacy manifest is read from a bundle containing a `syscalls` folder
- **THEN** the platform MUST be inferred as `SelfManaged` without rewriting the manifest
