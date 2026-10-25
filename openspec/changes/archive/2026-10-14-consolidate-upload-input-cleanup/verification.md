# First implementation verification

## Completeness

All code-side cleanup and compatibility cases are exercised. Native top layer 4, PR439, has head `elastic/esdiag:stack/pr330-upload-cleanup` and base `stack/pr330-scrub-traversal` (PR438), not main. It inherits the full implementation of all lower layers. The standalone evidence below is retained from before migration; final combined verification is required on this top head. The change remains active until stack review and required archival are complete.

## Correctness

- Baseline: 16 upload tests, one queued file/directory cleanup test and 18 runner tests passed before deletion mechanics changed.
- After: all 175 server library tests passed, including multipart rejection, exact rejection accounting, real staged files, owner claims, paused-clock five-minute expiry, directory substitution and queued directory cleanup.
- Actual HTTP/browser surface: a literal HTML-special filename rendered as text with no injected image. A real upload completed with 487 documents, zero document errors and complete outcome.
- Separate actual HTTP lifecycle: successful `/upload/submit` left the queued ZIP present; `/upload/process` completed successfully and the staged ZIP was absent afterward. This exercises successful-runner cleanup in addition to the permanent validation-rejection test.
- Duplicate, malformed-trailing and truncated HTTP submissions each returned 400 with no staged files left. Aggregate state afterward was two successful jobs plus exactly three failed jobs (`jobs.total=5`, `jobs.success=2`, `jobs.failed=3`, `active=0`, document errors zero).
- Workspace clippy returned zero errors, with four warnings outside changed files (`processor/mod.rs`, `setup.rs`, `system_integration.rs`). Changed-file language-server diagnostics were clear.

## Coherence

All nine design cases map to implementation and observable tests or HTTP smoke. Unqueued rejection cleanup remains file-only; queued input cleanup retains its existing file/directory policy. Rejection reasons/counts, enqueue transfer, owner-checked claim, expiry, runner cleanup and original filename/scrub selection remain unchanged. No synthetic JobInput, new filesystem adapter or executor. Nearby docs and changelog are intentionally unchanged for this internal refactor.

## Scope

Native strict OpenSpec validation is the syntax gate, not proof of behavior. Specs are intentionally skipped (`skip_specs: true`); the preserved design and existing requirements remain the compatibility contract. Archival is a later merge-completion step.

## Post-review verification for native PR439

- Copilot reviewed implementation `fc4c654a4c4c34294dfeeae5c78a24d267f50b1d` and recommended approval. The current-head helper verification reported a completed review, no review in flight, and zero unresolved Copilot-origin threads.
- Rechecked all nine design compatibility cases against the upload handler, cleanup policies, owner-checked queue claim, expiry, and request/file/queue assertions. All nine implementation tasks are complete. No implementation or behavior change was needed.
- Combined post-review checks on this implementation passed: 46 receiver library tests, 175 server library tests, and 30 integration tests across typed receiver, raw receiver, typed/streamed scrub, normalization export, and Logstash archive suites (251 total). Workspace clippy reported zero errors and the same four warnings in `processor/mod.rs`, `setup.rs`, and `system_integration.rs`. Cleanup, upload, and runner language-server diagnostics were clear.
- The actual CLI/browser/HTTP smoke already exercised this unchanged native implementation before review: scrub off/on each exported 803 documents with zero failures and verified two nodes; successful upload retained its staged ZIP until claim and removed it after completion. Duplicate, malformed-trailing, and truncated submissions each returned 400 with no staged files left. Literal HTML-special filenames rendered as text without an injected image. Aggregate HTTP counters were five jobs, two successes, three failures, zero active jobs, and zero document errors. These are retained observations, not a fresh post-review runtime run.
- Native strict validation passed. Artifact status reports no delta specs and intentionally skipped specs, so main-spec synchronization is unnecessary. Archive only this approved change with all artifacts preserved; lower-layer active changes and required completion gates remain independent blockers to merging the four-layer stack.

## Runner-level owner isolation review correction

Later review identified that the lifecycle test exercised Bob's claim through the queue method rather than the upload runner. The test now submits Bob's attempt through `run_upload_job`, observes the requested Job's failure card and decoded terminal processing/reset signals, and verifies Alice's queued request and staged bytes are unchanged. Alice can still claim the file, its claimed input survives unclaimed expiry, and runner cleanup removes it afterward. Rejection totals distinguish Bob's rejection from Alice's later validation failure.

The strengthened deterministic lifecycle test passed. This correction changes regression coverage only, not production ownership or cleanup behavior; no operator documentation or changelog change is needed for this test-only correction. The earlier review and runtime observations above retain their original implementation SHA.
