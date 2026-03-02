## Context

`esdiag collect` and `esdiag process` currently emphasize directory-oriented output, while legacy support diagnostics commonly produce a single zip artifact. The requested behavior adds a zip output mode that keeps current defaults intact but allows archive-first workflows for parity and easier artifact handling. The implementation must remain cross-platform and avoid shelling out to external zip binaries.

## Goals / Non-Goals

**Goals:**
- Add `--zip` output mode to `collect` and `process` CLI commands.
- For `collect`, support `--zip` as `Option<Path>` with default `.` and explicit output-directory targeting.
- Preserve existing diagnostic base naming and append `.zip`.
- Stream API output directly into archive entries while processing each API result (no temporary full directory output).
- Keep existing non-zip behavior unchanged and backward compatible.

**Non-Goals:**
- Replacing existing directory output as the default mode.
- Changing diagnostic content selection or API fetch semantics.
- Adding archive encryption, signing, or multipart archive splitting.

## Decisions

- **CLI contract for `collect --zip`**
  - Decision: Model `--zip` as optional path-like destination semantics where omission means current directory and explicit value means output directory.
  - Rationale: Matches requested UX while preserving deterministic output placement.
  - Alternative considered: `--zip <file>` exact path. Rejected because it diverges from requested Option<Path> directory behavior and naming convention.

- **Archive filename derivation**
  - Decision: Reuse the same base diagnostic name generation currently used for directory output and append `.zip`.
  - Rationale: Preserves naming consistency across modes and avoids introducing a second naming algorithm.
  - Alternative considered: New zip-specific naming prefix. Rejected because it breaks parity and discoverability.

- **Write path architecture**
  - Decision: Introduce zip-backed writer path in collection/process output layer so each API result is written into archive entries as it is produced.
  - Rationale: Satisfies "write directly to file" and avoids temporary materialization of the complete output tree.
  - Alternative considered: Continue writing to directory then bundle. Rejected because it violates requirement and doubles IO.

- **`process --zip` behavior**
  - Decision: When enabled, persist all API-call output into `{diagnostic}.zip`, with entry names matching existing per-API path conventions.
  - Rationale: Enables a single artifact from processed output while preserving internal file naming expectations.
  - Alternative considered: Per-API standalone zip files. Rejected because it fragments output and does not match requirement.

## Risks / Trade-offs

- **[Risk] Archive writer lifetime and error handling** -> Mitigation: Centralize finalization/flush in a single owner, ensure graceful close on early failures, and propagate structured errors.
- **[Risk] Concurrent writes to a single zip stream** -> Mitigation: Funnel writes through a serialized output stage or buffered channel to preserve archive consistency.
- **[Risk] Large diagnostics may increase single-file write contention** -> Mitigation: Keep existing fetch concurrency but decouple result production from archive-entry serialization.
- **[Risk] Path normalization differences across platforms** -> Mitigation: Normalize entry paths to forward-slash archive conventions and sanitize separators before writing.
