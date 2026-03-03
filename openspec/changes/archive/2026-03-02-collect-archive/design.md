## Context

`esdiag collect` and `esdiag process` currently emphasize directory-oriented output, while legacy support diagnostics commonly produce a single zip artifact. The requested behavior adds a zip output mode that keeps current defaults intact but allows archive-first workflows for parity and easier artifact handling. The implementation must remain cross-platform and avoid shelling out to external zip binaries.

## Goals / Non-Goals

**Goals:**
- Add `--zip` output mode to `collect` and `process` CLI commands.
- For `collect`, support `--zip` as a boolean mode switch while continuing to use the existing `output` positional argument for destination (`.` by default).
- For `process`, support `--zip` as optional destination directory semantics (`Option<Path>`, default `.` when flag is provided without an explicit value).
- Preserve existing diagnostic base naming and append `.zip`.
- Stream API output directly into archive entries while processing each API result (no temporary full directory output).
- Keep existing non-zip behavior unchanged and backward compatible.

**Non-Goals:**
- Replacing existing directory output as the default mode.
- Changing diagnostic content selection or API fetch semantics.
- Adding archive encryption, signing, or multipart archive splitting.

## Decisions

- **CLI contract for `collect --zip`**
  - Decision: Model `--zip` as a boolean mode selector; destination remains the existing `output` positional path used by `collect`.
  - Rationale: Keeps `collect` CLI simple and aligns with implemented behavior where `output` already determines destination.
  - Alternative considered: `--zip <path>` optional value. Rejected to avoid overloading `collect` destination semantics and to keep explicit separation between mode (`--zip`) and destination (`output`).

- **CLI contract for `process --zip`**
  - Decision: Model `--zip` as optional path-like destination semantics where omission means current directory and explicit value means output directory.
  - Rationale: `process` has no positional output directory argument for collection artifacts, so optional-path `--zip` is the least ambiguous destination control.
  - Alternative considered: boolean-only `--zip` with no destination control. Rejected because it prevents directing intermediate archive output.

- **Archive filename derivation**
  - Decision: Reuse the same base diagnostic name generation currently used for directory output and append `.zip`.
  - Rationale: Preserves naming consistency across modes and avoids introducing a second naming algorithm.
  - Alternative considered: New zip-specific naming prefix. Rejected because it breaks parity and discoverability.

- **Write path architecture**
  - Decision: Introduce zip-backed writer path in collection/process output layer so each API result is written into archive entries as it is produced.
  - Rationale: Satisfies "write directly to file" and avoids temporary materialization of the complete output tree.
  - Alternative considered: Continue writing to directory then bundle. Rejected because it violates requirement and doubles IO.

- **`process --zip` behavior**
  - Decision: When enabled, collect API-call output into a single archive using the same base naming convention as `collect` (for example, `api-diagnostics-<timestamp>.zip`), with entry names matching existing per-API path conventions.
  - Rationale: Preserves naming parity between `collect --zip` and `process --zip` flows.
  - Alternative considered: Per-API standalone zip files. Rejected because it fragments output and does not match requirement.

## Risks / Trade-offs

- **[Risk] Archive writer lifetime and error handling** -> Mitigation: Centralize finalization/flush in a single owner, ensure graceful close on early failures, and propagate structured errors.
- **[Risk] Concurrent writes to a single zip stream** -> Mitigation: Funnel writes through a serialized output stage or buffered channel to preserve archive consistency.
- **[Risk] Large diagnostics may increase single-file write contention** -> Mitigation: Keep existing fetch concurrency but decouple result production from archive-entry serialization.
- **[Risk] Path normalization differences across platforms** -> Mitigation: Normalize entry paths to forward-slash archive conventions and sanitize separators before writing.
