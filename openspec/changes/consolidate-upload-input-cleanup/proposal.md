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
- Branch `feature/consolidate-upload-input-cleanup`, local base `33357cf976b2881bbdf2002b3c6e3de7e244fa25` (reviewed PR330 tip). Future PR targets `main`, depends on https://github.com/elastic/esdiag/pull/330, and is independent of the two sibling simplification branches; overlapping `src/` paths create integration conflict risk, not a sibling dependency. After PR330 lands, rebase this branch onto `main`; if PR330 is squash/rebase-merged, replay only this child branch's commits onto the resulting main tip, excluding PR330's original commits. This planning change stays active until implementation and review are verified; do not sync/archive yet.
