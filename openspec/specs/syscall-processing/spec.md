# syscall-processing Specification

## Purpose

Define how ESDiag discovers, selects, and processes OS-command ("system call") output files in diagnostic bundles, such as `syscalls/sar.txt`, so that every syscall processor shares one catalog, one selection model, and one missing-file and failure policy. SAR is the first syscall processor. Collecting syscall output is out of scope here and belongs to a separate syscall collection capability.

## Requirements

### Requirement: Syscall source catalog
The system SHALL keep an ESDiag-owned, embedded catalog of syscall sources, separate from the REST `sources.yml` registries. Each entry SHALL be keyed by the matching `support-diagnostics` `diags.yml` command key (for example `sar`, `ss`, `netstat`) and SHALL declare:
- the bundle-relative output file (for example `syscalls/sar.txt`),
- its diagnostic-type membership tags,
- whether it is processable, and if so its processing weight and the applications whose processors implement it,
- whether it is a user-facing processing option.

The catalog SHALL be the only place a syscall output path is defined. Processors SHALL resolve file paths through it, never from hard-coded strings. Entries SHALL NOT carry REST `versions`. The catalog SHALL be shaped so that a later syscall collection capability can add per-OS command definitions to existing entries without renaming keys or changing processing behavior. The runtime SHALL NOT read `diags.yml`.

#### Scenario: Catalog entries for this change
- **GIVEN** the embedded syscall catalog
- **WHEN** it is loaded
- **THEN** it MUST contain a processable `sar` entry for `syscalls/sar.txt`, implemented for Elasticsearch, tagged `standard` and `support`
- **AND** it MUST contain non-processable `ss` and `netstat` entries for `syscalls/ss.txt` and `syscalls/netstat.txt`, used only as inputs to other processors

#### Scenario: Catalog and dispatch stay in sync
- **GIVEN** the syscall catalog and the per-application syscall dispatch tables
- **WHEN** the registry validation runs at startup or in tests
- **THEN** every processable entry implemented for an application MUST have exactly one registered processor for that application, and every registered processor MUST have a processable catalog entry

#### Scenario: Keys do not collide with REST sources
- **GIVEN** the syscall catalog and each product's `sources.yml`
- **WHEN** processing options are built
- **THEN** every syscall processing option MUST be addressed as `syscalls.<key>`, and no `syscalls.<key>` name MUST equal a REST source key

### Requirement: Syscall processing selection
Each processable syscall source implemented for an application SHALL be a processing option for that application, alongside its REST processing options, under the key `syscalls.<key>`. Syscall options SHALL never be required, so the user can always deselect them. Default membership SHALL come from the entry's tags: a diagnostic type includes `syscalls.<key>` by default when the entry carries that type's tag, and `custom` includes every option. When no processing selection is given at all, every processable syscall source for the application SHALL be processed in default mode.

The resolved processing selection SHALL record which keys the user explicitly requested, distinct from keys added by diagnostic-type defaults, required locking, or dependency resolution. A syscall source is **explicitly requested** only when its key appears in the user-supplied selection list. A selection submitted as the full checked list, as the web form and saved-job signals submit it, SHALL record as explicitly requested only keys outside the diagnostic type's defaults, so that resaving an unchanged job keeps its provenance. Selection validation against collected REST sources SHALL ignore `syscalls.*` keys, because ESDiag collection does not produce syscall output.

#### Scenario: Standard processing includes SAR by default
- **GIVEN** an Elasticsearch bundle processed with diagnostic type `standard` and no explicit selection
- **WHEN** the processing selection is resolved
- **THEN** `syscalls.sar` MUST be selected in default mode

#### Scenario: Minimal processing omits SAR
- **GIVEN** an Elasticsearch bundle processed with diagnostic type `minimal` and no explicit selection
- **WHEN** the processing selection is resolved
- **THEN** `syscalls.sar` MUST NOT be selected and `syscalls/sar.txt` MUST NOT be read

#### Scenario: User deselects SAR
- **GIVEN** a user-supplied Elasticsearch selection that omits `syscalls.sar`
- **WHEN** the diagnostic is processed
- **THEN** `syscalls/sar.txt` MUST NOT be read, and no SAR summary or event MUST be reported

#### Scenario: User explicitly requests SAR
- **GIVEN** a user-supplied Elasticsearch selection that lists `syscalls.sar`
- **WHEN** the processing selection is resolved
- **THEN** `syscalls.sar` MUST be selected and recorded as explicitly requested

#### Scenario: Option listing is per application
- **GIVEN** the processing option catalog for Elasticsearch, Kibana, and Logstash
- **WHEN** options are listed for each product
- **THEN** `syscalls.sar` MUST appear only for Elasticsearch, as a non-required option

#### Scenario: Collect job with SAR selected
- **GIVEN** a collect-and-process job whose processing selection lists `syscalls.sar`
- **WHEN** the selection is validated against the collected REST sources
- **THEN** validation MUST NOT fail because `syscalls.sar` was not collected

#### Scenario: Resaved job keeps default mode
- **GIVEN** a saved `standard` job whose selection includes `syscalls.sar` by default
- **WHEN** it is loaded into the job form and saved unchanged
- **THEN** `syscalls.sar` MUST remain selected and MUST NOT be recorded as explicitly requested

#### Scenario: Custom collect filters include SAR
- **GIVEN** a `custom` collect-and-process job with include filters and no processing selection
- **WHEN** its processing selection is derived from the collect filters
- **THEN** `syscalls.sar` MUST be selected

#### Scenario: Saved selection without explicit-request data
- **GIVEN** a saved job whose stored processing selection predates explicit-request tracking
- **WHEN** it is loaded
- **THEN** it MUST deserialize successfully, and its syscall sources MUST be treated as default mode

### Requirement: Missing syscall output
Whether a selected syscall source is missing SHALL be decided before its processor runs, in one shared step for every syscall processor. A source is missing when the receiver is not a bundle, when the bundle has no `syscalls/` directory, when the file is absent, or when the file has no non-blank lines.
- In default mode, a missing source SHALL be reported as missing with no warning or error event, and SHALL NOT affect the diagnostic outcome.
- When explicitly requested, a missing source SHALL be reported as missing with one warning event naming the expected file. It SHALL NOT be a processor failure and SHALL NOT fail the diagnostic.

#### Scenario: Default mode, file absent
- **GIVEN** an Elasticsearch bundle processed with type `standard` and no explicit selection, whose `syscalls/` directory has no `sar.txt`
- **WHEN** the diagnostic is processed
- **THEN** the SAR source MUST be reported as missing, no warning or error event MUST be recorded for it, and the diagnostic MUST succeed

#### Scenario: Default mode, no syscalls directory
- **GIVEN** an Elasticsearch bundle without a `syscalls/` directory, processed with type `standard`
- **WHEN** the diagnostic is processed
- **THEN** no SAR warning or error event MUST be recorded, and the diagnostic MUST succeed

#### Scenario: Explicit request, file absent
- **GIVEN** a user-supplied selection listing `syscalls.sar` and a bundle without `syscalls/sar.txt`
- **WHEN** the diagnostic is processed
- **THEN** the report MUST record one warning event for `syscalls.sar` naming `syscalls/sar.txt`, no processor failure MUST be counted, and the diagnostic MUST succeed

#### Scenario: Explicit request on a live-cluster diagnostic
- **GIVEN** a user-supplied selection listing `syscalls.sar` and a diagnostic received directly from a cluster API
- **WHEN** the diagnostic is processed
- **THEN** no file read MUST be attempted, and the report MUST record one warning event that `syscalls/sar.txt` is unavailable

#### Scenario: Empty file
- **GIVEN** a bundle whose `syscalls/sar.txt` contains only blank lines
- **WHEN** the diagnostic is processed in default mode
- **THEN** the source MUST be treated as missing with no warning, and with a warning if `syscalls.sar` was explicitly requested

### Requirement: Syscall processing failures
A syscall processor SHALL never fail the diagnostic. When a selected syscall file exists but cannot be read or parsed, the system SHALL record a warning event for that source, in default mode and when explicitly requested. It SHALL NOT count a processor failure. Documents exported before a mid-file read error SHALL be kept. The source's success verdict SHALL be recorded only when it exported at least one document or recorded no warning.

#### Scenario: Unparseable content in default mode
- **GIVEN** an Elasticsearch bundle processed with type `standard` whose `syscalls/sar.txt` contains `bash: sar: command not found`
- **WHEN** the diagnostic is processed
- **THEN** the report MUST record one warning event for `syscalls.sar`, no processor failure MUST be counted, every other processor MUST export normally, and the diagnostic MUST succeed

#### Scenario: Read error mid-file
- **GIVEN** a `syscalls/sar.txt` whose stream fails after several sections
- **WHEN** the diagnostic is processed
- **THEN** documents from the sections read before the error MUST be exported, one warning event MUST be recorded, and no processor failure MUST be counted

#### Scenario: Partial success
- **GIVEN** a `syscalls/sar.txt` with known sections and one unrecognized section
- **WHEN** the diagnostic is processed
- **THEN** the report MUST record the export success verdict and one warning for the unrecognized section

### Requirement: Syscall processing is independent of platform
Selected syscall sources SHALL be processed whenever the bundle contains their files, whatever the resolved `diagnostic.platform`. ECE-allocator and Kubernetes-node collections also produce `syscalls/`.

#### Scenario: Bundle with another platform
- **GIVEN** a bundle containing `syscalls/sar.txt` whose platform resolves to `ECE`
- **WHEN** it is processed with type `standard`
- **THEN** `syscalls.sar` MUST be processed
