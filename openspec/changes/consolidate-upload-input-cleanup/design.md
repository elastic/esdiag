## Context

See [proposal.md](proposal.md) for motivation and branch/PR lineage. `src/server/file_upload.rs::submit` accepts one `.zip` multipart file, keeps an optional original filename and staged path until parsing completes, then calls `ServerState::push_upload`. It currently repeats `remove_file` for duplicate, partial-write, and malformed-part cleanup; `JobInput::LocalArchive::cleanup_path` instead invokes `JobInput::cleanup` after a queued request is discarded, expires, or completes. `JobInput` already supports file and directory cleanup, missing-path success, and nonfatal errors; preserve that behavior. `diagnostic-workflow` is the existing inbound Load capability; `elastic-uploader` is outbound Send, not this upload. No behavioral delta is introduced (`skip_specs: true`).

## Goals / Non-Goals

**Goals:**
- Concentrate existing temporary-path deletion mechanics behind one narrow private function while keeping parser-owned rejection reason, status, and rejection count in the upload handler. The seam must serve both unqueued files and queued `JobInput` without requiring fabricated Job domain values.
- Keep staging path ownership obvious and tests at multipart request/response plus file/queue/stats outcomes; keep existing owner-isolation tests and original UI rendering.

**Non-Goals:**
- No new upload framework, public interface, filesystem trait/adapter with one implementation, second Job executor, or changes to Receiver Load or outbound Send.
- No cancellation/destructor guard, synchronous `Drop` deletion, or asynchronous destructor redesign. No new retry, telemetry, or failure-reclassification policy; nonfatal cleanup failure and `NotFound` success stay as-is.

## Decisions

### Reuse deletion mechanics, not Job domain construction

Extract shared missing-path and nonfatal deletion-error handling into one narrow private cleanup implementation in `src/server/mod.rs`. Preserve the caller's existing deletion policy: unqueued multipart rejection paths remain file-only `remove_file`, while queued `JobInput::cleanup` keeps its current metadata-based file/directory selection. A small private policy choice can distinguish these two real call paths; it is not a new filesystem trait or adapter. An upload path replaced by a directory must not gain recursive-delete permission merely because the mechanics are shared. Keep `JobRequest::cleanup` as the queued-input entry point and retain runner, discard and expiry callsites. Do not construct a synthetic JobInput solely to remove a partial file. Cleanup remains nonfatal and cannot replace response status/reason.

**Deletion test:** Deleting the private cleanup implementation would restore repeated missing-path and nonfatal-error handling across rejection and queued cleanup paths. File-versus-directory selection remains a real queued-input responsibility; file-only rejection stays file-only. This modest depth gives leverage to both call paths and locality to cleanup corrections without a public interface or hypothetical second adapter.

### Explicit upload path state and transfer

The state progression is **no path** → **partially written** (`File::create` may succeed before chunks fail) → **fully staged but unqueued** → **queued `JobRequest`** → **claimed or expired** → **cleaned**. A failure in staging cleans its known target path even when file creation failed; a duplicate or malformed subsequent field cleans the fully staged but unqueued path. The handler owns deletion before queue insertion; successful enqueue transfers it to `JobInput::LocalArchive.cleanup_path` and the handler must not subsequently delete it. Claim is owner-checked (`pop_job_request_for_owner`); an unsuccessful claim leaves the real Owner's request/path intact. The existing five-minute timer removes only an unclaimed request using `pop_job_request` and cleans its Job input; a successful claim removes that request, making the timer a no-op. The runner cleans claimed Job input on validation rejection or completed execution; rejection accounting remains at the same request rejection sites. No new type-state types or external seam are introduced: these are ownership states in the existing upload module and queue.

Keep parsing all fields before enqueue, at most one file, `BAD_REQUEST` and current distinct failure message per duplicate, missing file/name, invalid `.zip`, staging error, and malformed multipart. An unknown well-formed field remains ignored; an unknown malformed trailing field must still fail the full parse and delete staged input. An explicitly present `scrubbed=false` stays `Some(false)` and overrides filename auto-detection; absent checkbox stays `None`. Preserve the original filename in queued `JobInput` and Receiver auto-detection while escaping only its rendered HTML; retain current Askama and Datastar conventions. ADR-0004 keeps Load in the Receiver and the one executor, ADR-0008 keeps Owner isolation, and ADR-0016 does not let cleanup hide a request rejection.

### Behavioral compatibility cases (no new spec requirements)

- **GIVEN** a completed single `.zip` part and no checkbox, **WHEN** `/upload/submit` accepts the multipart body, **THEN** it responds OK, queues one Owner-associated Job with original filename and `None` scrub override, and keeps the staged bytes until claim/expiry or Job cleanup.
- **GIVEN** a completed `.zip` part and `scrubbed=false`, **WHEN** submission finishes, **THEN** queued override is `Some(false)` even for a filename with the `scrubbed` token; the active UI channel's explicit setting takes precedence over auto-detection without modifying CLI selection.
- **GIVEN** a staged first file, **WHEN** a second file part is reached, **THEN** the existing duplicate-file `BAD_REQUEST` is returned, the staged file is absent, no Job is queued, and failed/total rejection stats each advance once.
- **GIVEN** an incomplete file body, **WHEN** staging fails, **THEN** the current staging-error `BAD_REQUEST` is returned and any partially written path is gone; no Job is queued and rejection stats each advance once.
- **GIVEN** a fully staged file, **WHEN** a trailing `scrubbed` or unknown field is malformed or the next part has malformed headers, **THEN** the current malformed-data `BAD_REQUEST` is returned and the staged path is gone, without enqueue or duplicate rejection counting; a well-formed unknown field remains ignored.
- **GIVEN** an original filename containing HTML-special text, **WHEN** a valid upload responds, **THEN** response HTML renders the filename literally escaped while the queued Job retains the unchanged original filename and bytes.
- **GIVEN** a queued staged upload for Owner A, **WHEN** Owner B attempts to claim it, **THEN** no Job or staged path is removed; **WHEN** Owner A claims it, **THEN** the five-minute timer does not delete the claimed input and runner cleanup removes it when the Job concludes. **GIVEN** that request remains unclaimed for five minutes, **WHEN** expiry takes it instead, **THEN** its file is removed. These alternatives are exclusive, not competing owners of the file.
- **GIVEN** an already absent input path or a non-`NotFound` filesystem cleanup error, **WHEN** existing cleanup runs, **THEN** absence is harmless and other deletion failures are nonfatal; the upload rejection, queue state, and Job result are not rewritten by cleanup.
- **GIVEN** an unqueued upload staging path has become a directory, **WHEN** multipart rejection cleanup runs, **THEN** it retains file-only deletion behavior, leaves that directory intact and reports cleanup failure nonfatally; queued Job input directory cleanup remains supported as before.

## Risks / Trade-offs

- [Risk] Moving path deletion can accidentally clean a successfully enqueued/claimed Job's file, or omit partially created files → Keep transfer at successful queue insertion only; verify real file and queue states through multipart/owner tests, including trailing malformed parts and partially written files.
- [Risk] Unifying cleanup could change error status, double-count rejection, or alter HTML/filename/scrub precedence → Keep parsing and rejection branches in place; compare response status/body, stats delta, queue contents, original filename, and staged bytes using observable tests rather than logging or source-text assertions.
- [Risk] Lower-layer fixes can invalidate the top layer → This PR is based on scrub traversal and includes typed archive loading plus scrubbing. Propagate fixes upward with the native stack's cascading rebase, then rerun upload/owner tests, receiver contracts and the actual combined server/CLI smoke before merge. Verify predecessor bases and GitHub stack metadata; sibling PRs against main are not a substitute for this linear stack.

## Migration Plan

No data migration, runtime flag, or rollout mechanism. Implement the private helper and replace only duplicate cleanup branches, preserve queue handoff and Job executor, then run targeted upload/owner regression checks and an actual server-upload smoke check. Reverting the implementation restores previous duplicated deletion paths without migrating persisted data. Keep this change active through review; sync/archive only after verified implementation.
