## Context

See [proposal.md](proposal.md#why) for motivation and [proposal.md](proposal.md#branch-and-pr-dependency) for the stack relationship. At the recorded PR330 base, `ArchiveFileReceiver::get` (`file.rs:83–127`) and `ArchiveBytesReceiver::get` (`bytes.rs:50–97`) repeat the same candidate iteration and scrub-aware deserialization. `archive.rs:27–84` already shares streaming implementation across those two adapters.

The existing `Receive` interface is the test surface. `MissingSource` distinguishes absent or whitespace-only data from unreadable or malformed data. `resolve_archive_path` also owns nested working-directory resolution, archive separator normalization, and the legacy double-slash fallback; this refactor must reuse it rather than rediscover those rules.

## Goals / Non-Goals

**Goals:**

- One typed archive Load implementation for ordered source candidates, empty-input detection, normalization, deserialization, and final absence handling.
- Keep both concrete archive adapters and existing consumer-visible results, errors, and bounded-memory normalization.
- Gain depth at the existing receiver interface: caller knowledge stays fixed while locality improves across both adapters.

**Non-Goals:**

- No generic filesystem abstraction, new receiver trait, new adapter, or directory-reader rewrite.
- No merging of raw String, eager typed, and incremental streaming contracts.
- No change to source definitions, scrub detection/filename hints, node lookup, Job execution, concurrency, tracing configuration, or processor typestate.
- No schema, command, HTML, or output-format change.

## Decisions

### Keep the shared implementation inside the archive module

Use the existing concrete `ZipArchive<R>` shape, generic over its current `Read + Seek` storage, to share typed loading just as streaming is already shared. Both adapters retain their archive handles and source-context initialization. The shared implementation operates on the already-acquired archive access rather than cloning payloads or creating another synchronization policy.

The implementation must not add a whole-entry String, `serde_json::Value`, runtime-dispatched storage trait, or a new allocation solely for delegation. Preserve the existing reader route through `with_normalized_json_reader`. Adapter-specific entry-opening errors remain mapped as they are today through a narrow private error-mapping choice; do not erase the distinction merely to obtain identical code.

**Alternative rejected:** a new public storage interface implemented by file, memory, and directory adapters. It widens the interface and mixes different path/error semantics to remove a much smaller duplicated implementation. Two archive adapters already make the internal seam real; they do not justify a new external seam.

### Preserve candidate and failure transitions exactly

The shared read sequence is:

1. Resolve the data source's ordered candidate paths using the existing source context.
2. Resolve each candidate through `resolve_archive_path`; remember a resolution failure and try the next candidate.
3. Open the resolved archive entry. An opening/I/O failure remains a failure, not absence.
4. Use `source_has_json` to skip JSON whitespace without materializing a second payload. A whitespace-only entry records `MissingSource::Empty` and allows the next candidate.
5. Normalize only when the existing scrub flag and supported-file predicate permit it; deserialize through the existing reader.
6. A deserialization error terminates the read with source context; it must not fall through to a later candidate or become `MissingSource::Empty`.
7. Return the first successfully parsed value. If no candidate succeeds, return the last recorded candidate failure; only an empty candidate list produces `MissingSource::NoCandidates`.

These are Load control-flow transitions, not a new typestate model. Processor lifecycle transitions remain unchanged, including the established Ready → Processing → Completed/Failed progression. No new Receiver, Process, or Export trait interface is introduced; ADR-0004's stage placement and single executor remain intact.

**Alternative rejected:** treating every serde EOF error as an empty source. A truncated JSON value is malformed even when serde classifies the parse error as EOF.

### Retain distinct lifetime and storage responsibilities

Keep the archive lock/guard and entry reader alive through normalization and deserialization. Keep normalized temporary content alive inside `with_normalized_json_reader`; do not return a borrowed reader whose backing file is dropped. Preserve each adapter's existing source product, nested working directory, and scrub flag when it creates a nested receiver.

This change does not rewrite `get_stream_from_archive` or the raw read route. Their existing seams already expose different consumer contracts, and merging them would move rather than delete complexity.

### Verify observable contracts instead of helper delegation

Use equivalent synthetic bundles as a real file archive and as an in-memory archive, with isolated fixture state. Tests call the existing typed receiver interface and assert parsed fields or error categories. A private shared helper is not itself the test surface.

Keep existing Load → Process → Export assertions for normalized node addresses and stable joins. Raw receiver tests remain useful regression coverage but do not prove typed candidate behavior. Add only uncovered consumer-visible cases; do not create forwarding, source-text, logging-wording, or not-throw tests.

## Compatibility Acceptance Cases

### Ordered candidate selection

- **GIVEN** both archive adapters contain two candidate source entries with distinguishable valid values
- **WHEN** the same typed data source is loaded from each adapter
- **THEN** both return the first candidate's parsed value in the existing declared order.

### Empty candidate permits a later candidate

- **GIVEN** the first candidate contains only JSON whitespace and a later candidate contains valid data
- **WHEN** either archive adapter performs the typed read
- **THEN** it returns the later candidate without classifying the empty first entry as malformed.

### Corrupt candidate is not silently bypassed

- **GIVEN** the first present candidate contains truncated or invalid JSON and a later candidate is valid
- **WHEN** either archive adapter performs the typed read
- **THEN** it reports the parse failure rather than selecting the later candidate or returning a missing-source error.

### Absence classification stays typed

- **GIVEN** no candidate succeeds
- **WHEN** either archive adapter completes candidate iteration
- **THEN** it retains the last existing resolution/empty-source failure, including its path
- **AND** an empty candidate list produces `MissingSource::NoCandidates` with the data source name.

### Nested archive compatibility

- **GIVEN** a nested diagnostic resolves through the existing working-directory and legacy double-slash rules
- **WHEN** a typed source is loaded through either archive adapter
- **THEN** it resolves the same entry and preserves the nested receiver's scrub flag.

### Scrub mode preserves typed values

- **GIVEN** a supported synthetic Elasticsearch source contains `ip: "512.768.1024.1280"`, a port-bearing address, and a non-address identifier
- **WHEN** either adapter loads it with scrub mode enabled
- **THEN** the pure address is `2.3.4.5`, the permitted port is retained, and the identifier's value and JSON type are unchanged
- **AND** explicit scrub disable preserves the original parsed address values.

### Excluded files and read failures

- **GIVEN** the source is an excluded manifest/version file or its reader encounters an I/O failure
- **WHEN** either adapter loads it
- **THEN** excluded content is not normalized and I/O failures are not reclassified as absent sources.

## Risks / Trade-offs

- **[Opening-error behavior differs between adapters]** → Preserve adapter-specific error mapping privately; tests assert meaningful failure classification and source context, not incidental wording.
- **[Candidate handling accidentally changes]** → Exercise valid-first, empty-first, malformed-first, all-missing and no-candidates outcomes through both adapters.
- **[Generic sharing becomes a shallow public module]** → Keep the shared implementation private to archive loading and keep the Receiver interface unchanged.
- **[Lifetime/lock behavior regresses]** → Retain existing lock scope and normalized-file ownership; run actual CLI processing on both scrub-enabled and disabled synthetic archive inputs.
- **[Sibling changes touch the same archive files later]** → This change depends only on PR330. Rebase locally when another sibling lands, preserving its normalization implementation rather than introducing a dependency or duplicate helper.

## Migration Plan

1. Add only missing contract coverage before changing typed loading.
2. Move shared typed orchestration into the archive module and migrate both adapters in one cutover.
3. Delete the duplicated implementation; preserve the existing raw/streaming routes and caller interface.
4. Run targeted tests, lint, and a CLI archive-processing smoke; compare exported node identity/address outcomes with the fixture expectations.
5. Inspect nearby implementation docs for architectural wording that needs updating. Behavior-only operator docs and changelog remain unchanged unless an actual observable correction is separately identified and approved.
6. Keep this change active during implementation and PR review. After verified implementation, archive it with the declared no-delta rationale; do not invent a main-spec edit. Apply the PR330 replay rule recorded in the proposal before merge to `main`.

No persisted data or user configuration migration is required. Rollback is a normal revert of the child implementation commit; it does not alter PR330's behavior or history.
