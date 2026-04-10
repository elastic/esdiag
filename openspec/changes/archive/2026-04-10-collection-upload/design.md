## Context

`esdiag collect` already performs the hard part of this workflow: it resolves a known host, generates a timestamped archive filename, writes the collected bundle locally, and returns the final archive path from the collector. Separately, `esdiag upload` already knows how to normalize an upload id or URL and send a raw archive to Elastic Upload Service. The gap is only in CLI orchestration: users who want "collect, then upload" must currently discover the generated filename and invoke a second command manually.

This change is limited to the CLI path. It does not add a new receiver, processor, exporter, or uploader protocol. The design should reuse the existing collection result path and uploader helper so the convenience flag behaves like a built-in `collect && upload` sequence without duplicating upload logic.

## Goals / Non-Goals

**Goals:**
- Add an optional `--upload` argument to `esdiag collect` while keeping `-u` as the short form of `--user`.
- Upload the exact archive emitted by the collect step after a successful collection run.
- Reuse the existing Elastic Upload Service uploader implementation and upload id normalization behavior.
- Keep the current collect-only behavior unchanged when the flag is absent.
- Make failure semantics match a logical `collect && upload`: no upload attempt on collect failure, and a failed upload surfaces as a failed command after the archive has been written locally.

**Non-Goals:**
- Changing the `esdiag upload` protocol or service API behavior.
- Introducing a new archive naming scheme for collection output.
- Adding a second convenience mode that skips writing the local archive.
- Expanding this change into workflow or web UI behavior.
- Adding extra upload configuration flags beyond the requested `upload_id` handoff.

## Decisions

1. **Add upload handoff directly to the collect command**
   - Decision: Extend `Commands::Collect` with an optional `upload_id` field exposed as long-form `--upload`, while preserving the existing `-u` shorthand for `--user`.
   - Rationale: The request is specifically a convenience affordance on `collect`, so the CLI surface should express the follow-up action at the point where the user already chooses the host and output location.
   - Alternatives considered:
     - Require users to keep using `esdiag upload` as a second explicit command: rejected because it does not solve the generated-filename problem.
     - Add a new combined subcommand such as `collect-upload`: rejected because it duplicates most of the existing collect interface.

2. **Use the collector's resolved archive path as the upload input**
   - Decision: After `collector.collect().await?` succeeds, the command SHALL use `CollectionResult.path` as the file path passed into the uploader helper.
   - Rationale: The collector already returns the actual emitted archive path, including the runtime-generated filename. Reusing that value avoids guessing, recomputing timestamps, or inferring output paths from identifiers.
   - Alternatives considered:
     - Reconstruct the filename from the `Identifiers` metadata: rejected because that is more brittle than using the authoritative result path.
     - Scan the output directory for the newest archive: rejected because it is ambiguous and race-prone.

3. **Reuse the existing uploader helper instead of shelling out**
   - Decision: The collect command SHALL call `uploader::upload_file` directly with the resolved archive path and provided `upload_id`.
   - Rationale: This preserves one implementation of upload id normalization, upload validation, chunking, and finalize behavior.
   - Alternatives considered:
     - Spawn a nested `esdiag upload` subprocess: rejected because it duplicates CLI parsing and complicates error handling.
     - Copy upload logic into the collect branch: rejected because it would create two upload code paths to maintain.

4. **Match `&&`-style sequencing and error behavior**
   - Decision: Upload SHALL only start after collection completes successfully. If upload fails, the overall `collect` command SHALL return an error while leaving the collected archive on disk.
   - Rationale: The user explicitly described the desired behavior as equivalent to `collect && upload`, and preserving the local archive on upload failure gives the user a recoverable artifact for manual retry.
   - Alternatives considered:
     - Treat upload as best-effort and still return success: rejected because it would hide support-delivery failures.
     - Delete the archive after a failed upload: rejected because it would destroy the artifact needed for retry or inspection.

5. **Keep upload configuration narrow for this change**
   - Decision: `collect --upload` SHALL reuse the uploader's default API base URL and SHALL not add a collect-specific override flag in this proposal.
   - Rationale: The request only asks for `upload_id` handoff. Keeping the change narrow reduces CLI surface area and keeps the convenience path aligned with the common case.
   - Alternatives considered:
     - Add `--upload-api-url` to collect immediately: deferred because it is not required for the requested workflow.

## Risks / Trade-offs

- **[Risk] Users may interpret `collect --upload` as "upload without saving locally"** -> **Mitigation:** document that collection still writes the archive locally first and then uploads that file.
- **[Risk] Upload failures occur after a potentially long collect run** -> **Mitigation:** return a clear error after collection while preserving the generated archive path on disk for retry.
- **[Risk] CLI behavior could drift between `collect --upload` and `upload`** -> **Mitigation:** reuse the same uploader helper rather than introducing a second implementation path.
- **[Risk] Future changes to collect output handling could break upload handoff** -> **Mitigation:** rely on `CollectionResult.path` as the stable contract for downstream actions.

## Migration Plan

1. Extend the `collect` CLI parser with the optional upload id argument.
2. After successful collection, call the uploader helper with the resolved archive path when the flag is present.
3. Add CLI parsing tests and command-path tests for collect-only, collect-plus-upload success, and collect-plus-upload failure.
4. Update user-facing documentation for the convenience flag.

Rollback strategy:
- Remove the `--upload` argument from `collect`.
- Restore collect to a pure local archive creation command without changing the standalone `upload` command.

## Open Questions

- None.
