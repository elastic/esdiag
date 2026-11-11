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
- Native layer 4: [PR439](https://github.com/elastic/esdiag/pull/439), head `elastic/esdiag:stack/pr330-upload-cleanup`, targets `stack/pr330-scrub-traversal` ([PR438](https://github.com/elastic/esdiag/pull/438)). It includes all three lower layers and is the combined implementation verification surface. The native chain is main ← scrubbing ← typed loading ← scrub traversal ← upload cleanup; only scrubbing targets main. Review this layer's immediate-parent diff and satisfy review, verification and archival gates before a native top-layer merge, which can atomically land all four PRs.
