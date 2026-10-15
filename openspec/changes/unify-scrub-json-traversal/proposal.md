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
- Native layer 3: [PR438](https://github.com/elastic/esdiag/pull/438), head `elastic/esdiag:stack/pr330-scrub-traversal`, targets `stack/pr330-typed-loading` ([PR437](https://github.com/elastic/esdiag/pull/437)). It inherits typed loading and the reviewed scrubbing parent `33357cf976b2881bbdf2002b3c6e3de7e244fa25`; upload cleanup is the next layer above it. Only the bottom scrubbing PR targets main. Review the immediate-parent diff, propagate lower-layer fixes upward, and complete verification plus required archival before a native top-layer merge. All four branches live in upstream, not forks.
