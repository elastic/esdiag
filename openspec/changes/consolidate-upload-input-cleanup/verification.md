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
