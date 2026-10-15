## Why

File-backed and in-memory archive adapters repeat typed Bundle Load orchestration, so PR330 had to add scrub handling to both implementations. Concentrating that implementation behind the existing Receiver interface improves locality and gives both adapters leverage without adding caller knowledge.

## What Changes

- Consolidate ordered candidate resolution, empty-source detection, scrub-aware deserialization, and final missing-source handling for typed archive reads inside the archive module.
- Retain the file and memory adapters, their concrete archive access, existing opening-error behavior, and the existing Receiver interface.
- Follow the existing shared streaming-read implementation as a factoring precedent; do not combine typed, streaming, and raw read contracts.
- Verify the retained contracts through typed receiver reads and Load → Process → Export, not private helper delegation assertions.
- Remove the duplicated typed-read implementation after every adapter uses the shared implementation; do not retain an obsolete compatibility path.

## Capabilities

### New Capabilities

None. This is an internal refactor, not a new capability.

### Modified Capabilities

None. Existing behavior in `malformed-ip-normalization`, `streaming-deserialization`, `version-dependent-sources`, and `included-diagnostic-jobs` remains unchanged. Metadata declares `skip_specs: true`; compatibility cases live in the design rather than a fabricated requirement delta.

## Impact

- **Products:** all application diagnostics supported by the existing archive adapters; Elasticsearch scrubbed diagnostics are the primary changed-path fixture. Product coverage does not expand.
- **Surfaces:** core Load implementation consumed by CLI `process` and web jobs. No CLI arguments, rendered HTML, public receiver traits, Job stages, Process logic, or Export contracts change.
- **Files:** `src/receiver/archive.rs`, `src/receiver/archive/file.rs`, `src/receiver/archive/bytes.rs`; focused receiver tests and existing synthetic scrub integration coverage.
- **Dependencies:** no new crate, storage abstraction, filesystem adapter, or processor lifecycle change. Two real archive adapters already justify the internal seam.
- **Architecture:** depth increases by hiding duplicated Load sequencing; locality improves because error classification and scrub orchestration change in one module.

## Branch and PR Dependency

- Branch: `feature/consolidate-typed-bundle-loading`.
- Starting commit: `33357cf976b2881bbdf2002b3c6e3de7e244fa25`, the locally reviewed PR330 tip.
- Future PR target: `main`; dependency: [elastic/esdiag PR330](https://github.com/elastic/esdiag/pull/330).
- This is a sibling of `feature/unify-scrub-json-traversal` and `feature/consolidate-upload-input-cleanup`; neither sibling is a prerequisite.
- Before PR330 merges, comparison with `main` includes its inherited commits. Review this proposal's own delta against the recorded PR330 base.
- After PR330 lands, replay only this branch's child commits onto the resulting `main`. If PR330 was squash- or rebase-merged, use the recorded original parent commit as the cut point instead of replaying PR330 itself.
- Planning only: do not implement, open a PR, synchronize specs, or archive this change during proposal creation. Keep it active through later implementation and review; synchronize applicable specs and archive only after implementation verification.
