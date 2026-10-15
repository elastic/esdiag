## Why

The Web UI Load upload handler independently deletes duplicate, partially written, and malformed multipart files even though `JobInput` already owns staged-input cleanup after enqueue. Consolidating the deletion mechanics at the existing seam reduces duplicated failure handling without changing request outcomes or Job execution.

## What Changes

- Reuse private cleanup mechanics for unqueued upload paths and queued `JobInput::LocalArchive` paths, leaving multipart parsing and rejection decisions in the upload handler.
- Preserve the current owner-checked claim, five-minute unclaimed Job expiry, original filename and HTML escaping, scrub override precedence, and nonfatal cleanup failures. No new upload framework, filesystem adapter, executor, or public interface is proposed.
- Exercise upload HTTP request/response and real staged-file/queue outcomes in permanent tests; retain existing useful rejection, filename-injection, and owner-isolation coverage. This is a behavior-preserving refactor, not a new capability.

## Capabilities

### New Capabilities

None. The change has `skip_specs: true` because it adds no externally observable requirement.

### Modified Capabilities

None. Existing `diagnostic-workflow` (Web UI Collect → Upload), `malformed-ip-normalization` (channel-local scrub selection), and Owner policy requirements remain unchanged. `elastic-uploader` is outbound Send, not this inbound Load surface; no narrower upload-input capability exists in the inspected main specs.

## Impact

- Target: the Web UI inbound Load path for existing diagnostic bundles from any supported Elastic Stack application/platform; no product-specific collection change. CLI, Receiver behavior, and core Job executor behavior stay unchanged.
- Future implementation scope: `src/server/file_upload.rs` staging/rejection, `src/server/mod.rs` `JobInput`/`JobRequest` cleanup and queue ownership, `src/server/job_runner.rs` existing cleanup callsites, and relevant upload/owner tests. Existing Askama-rendered HTML and Datastar signals remain intact.
- Branch `feature/consolidate-upload-input-cleanup`, local base `33357cf976b2881bbdf2002b3c6e3de7e244fa25` (reviewed PR330 rebase). The PR targets `elastic/esdiag:main` (`upstream/main`), depends on https://github.com/elastic/esdiag/pull/330, and is independent of its two sibling simplifications. Review, verify and complete required archival for all four PRs before one coordinated merge batch; overlapping source paths are integration risks, not sibling dependencies. Preserve parent ancestry where possible; if its merge rewrites commits, replay only this child's commits and re-verify within the same batch, not as deferred implementation or a later release.
