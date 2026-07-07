---
status: accepted
---

# ESDiag owns its collection definitions; support-diagnostics is a reconciliation input

ESDiag's per-product `sources.yml` files are owned by ESDiag and shaped to its own
model — not mirrored from, or bound at runtime to, `support-diagnostics`. Upstream
(`elastic-rest.yml` for API sources, `diags.yml` for OS-command sources) is treated
as a *reconciliation input*: a source of updates we diff into our own definitions,
not an authority we track verbatim.

## Context

ESDiag has already ported support-diagnostics' format — its `sources.yml` is
version-gated the same way (`get_url(version)` resolving semver ranges to
per-version queries). The genuinely valuable, hard-to-reproduce asset upstream is
that **per-version compatibility knowledge** (and the OS-command definitions),
maintained on every release. So the decision is not about file *shape* (already
ours) but about how tightly we track upstream *content*.

## Considered options

- **Mirror upstream verbatim / bind at runtime.** Rejected: forces ESDiag's
  registry into upstream's two-file, API-vs-command split and blocks the moves this
  review depends on — one transport-neutral `data source`, `Platform`/`Application`
  tagging, six-stage metadata, and correcting entries we disagree with.
- **Own from scratch, ignore upstream.** Rejected: throws away the per-version
  compatibility knowledge and the OS-command catalogue, which we would then have to
  reproduce and maintain against every release ourselves.
- **Own the definitions, reconcile from upstream (chosen).** Keep full control of
  shape and content; pull upstream's version-gating and command updates in through a
  reconciliation script that *overlays* the upstream files into ESDiag's, as a
  field-level merge.

## Consequences

- **Reconciliation is a required, recurring discipline — not optional.** It must be
  performed on **every application release** (a new Elasticsearch / Kibana /
  Logstash version can add or change endpoints and their version-gating) **and every
  support-diagnostics release** (upstream may revise definitions or OS commands).
  Without an owner and a cadence, version-gating silently goes stale — new endpoints
  missed, changed queries not updated — which is the primary risk this decision
  accepts.
- **Reconciliation is a field-level overlay, never a copy.** The script merges
  upstream's `versions`/paths and OS-command definitions *into* ESDiag's files while
  preserving ESDiag-only enrichments (`weight`, platform/application tags,
  streamable). A blind copy would wipe the hand-tuned concurrency weights (ADR-0005),
  so the merge must know which fields are ESDiag's.
- **The overlay normalizes semver at the boundary, letting the runtime drop its
  version-compatibility parser.** Upstream ranges use a Java/NPM semver dialect that
  does not exactly match the Rust `semver` crate, which forced a custom
  compatibility parser at runtime. Converting ranges into native Rust `semver` form
  *during reconciliation* means ESDiag's stored `sources.yml` is already in its own
  dialect, so the runtime uses stock `semver::VersionReq` and the shim is removed.
  The impedance is absorbed once, at the boundary, instead of on every parse.
- ESDiag may deliberately diverge from upstream (add/remove/correct sources); such
  divergences must be noted so reconciliation does not silently revert them.
- The runtime already supports this posture: definitions are embedded per product
  and overridable via `--sources`; no code depends on upstream files directly.
