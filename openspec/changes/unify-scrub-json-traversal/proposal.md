## Why

Elasticsearch scrub normalization currently traverses JSON twice in unrelated ways: raw Load materializes and recurses through `serde_json::Value`, while typed/streamed Load uses a private token/context stack. The address rules are already shared; one validated field-scope traversal would improve locality and reduce the risk of diverging array/object scoping without making typed/streamed Load pay for a payload-sized tree.

## What Changes

- Replace the two normalization traversals with one private, validating traversal in the receiver module. Preserve the existing address allowlist and transformation rules, receiver-first ordering, per-file transformed counts, and debug intent.
- Preserve distinct consumer interfaces: raw archive-file, archive-bytes, and directory Load returns `String` (required output allocation); typed and streamed Load uses the existing buffered temporary-file adapter and keeps that file alive through deserialization. Do not force raw through a temporary file or turn typed/streamed content into a whole-payload `String`/`Value`.
- Retain exact pass-through for disabled or unsupported content; keep supported-file selection for `.json` except `diagnostic_manifest.json` and `version.json`. Supported scrubbed raw output is JSON-semantic, not byte-stable for insignificant whitespace, escaping, or key order; no new acceptance rule is intended for malformed JSON.
- Preserve the existing raw `serde_json::Value`-compatible acceptance, JSON numeric values/types and last-key-wins duplicate-key interpretation without materializing a whole-payload `Value`. Characterize representation details during implementation; do not silently change compatibility to accommodate a token copier.

## Capabilities

### New Capabilities

None. This is a behavior-preserving implementation refactor (`skip_specs: true`).

### Modified Capabilities

None. Existing `malformed-ip-normalization` and `streaming-deserialization` requirements remain authoritative and unchanged; the implementation and verification cases in the design preserve them rather than adding a delta spec.

## Impact

- Target: Elasticsearch diagnostic JSON in local Bundle Load. The affected interface is the receiver path used by CLI `process` and Web UI upload; no CLI flags, upload controls, processor behavior, public receiver interfaces, or Job executor transitions change.
- Implementation locality: `src/receiver/archive/scrub.rs`; existing file/bytes/directory raw callers and typed/streaming seams remain the acceptance surfaces. No new dependency or public trait framework is proposed.
- Planning branch `feature/unify-scrub-json-traversal` starts from local PR330-tip commit `33357cf976b2881bbdf2002b3c6e3de7e244fa25`; future PR targets `main` and depends on [PR330](https://github.com/elastic/esdiag/pull/330). This branch is independent of sibling architecture proposals (including archive read consolidation), not stacked on them. Once PR330 lands, rebase this branch onto `main`; if PR330 was squash- or rebase-merged, replay only this branch's child commits rather than carrying PR330 commits into the PR.
