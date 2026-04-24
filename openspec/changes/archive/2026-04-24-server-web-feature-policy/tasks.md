## 1. Policy Model

- [x] 1.1 Replace `RuntimeModePolicy` with `ServerPolicy` while preserving existing runtime-mode decision methods.
- [x] 1.2 Add typed `WebFeature` and `WebFeatureSet` parsing for `advanced` and `job-builder`.
- [x] 1.3 Add `serve --web-features` and resolve web features with CLI override first, then `ESDIAG_WEB_FEATURES`, then runtime-mode defaults.
- [x] 1.4 Ensure unknown web feature values fail startup with an error that includes the invalid value and the known values.
- [x] 1.5 Add unit tests for policy defaults, CLI-over-env precedence, authoritative explicit lists, empty lists, unknown feature errors, desktop user defaults, and service-mode safety composition.

## 2. Routing And Templates

- [x] 2.1 Mount the Advanced workflow page at `/advanced` using `ServerPolicy::allows_advanced`.
- [x] 2.2 Remove `/workflow` route exposure without redirect and add direct URL tests proving `/workflow` is unmounted.
- [x] 2.3 Rename Rust handlers, templates, CSS identifiers, docs, and user-facing copy from workflow terminology to Advanced where they refer to the web page.
- [x] 2.4 Gate `/jobs` and `/jobs/saved*` web routes with `ServerPolicy::allows_job_builder`.
- [x] 2.5 Pass explicit feature booleans to shared templates and update header navigation to render Advanced and Job Builder links from policy decisions.
- [x] 2.6 Add route tests covering nav visibility and direct URL access for default user mode, explicit `advanced,job-builder`, empty web features, and service mode.

## 3. Documentation And Release Notes

- [x] 3.1 Update web/runtime documentation to describe `serve --web-features`, `ESDIAG_WEB_FEATURES`, precedence, valid values, defaults, and authoritative behavior.
- [x] 3.2 Update references from `/workflow` to `/advanced` and from workflow page naming to Advanced where user-facing.
- [x] 3.3 Update saved-jobs documentation to clarify CLI saved jobs remain available when Job Builder web UI is disabled.
- [x] 3.4 Add a changelog entry for runtime web feature gating and the Advanced route rename.

## 4. Verification

- [x] 4.1 Run `cargo test`.
- [x] 4.2 Run `cargo clippy`.
- [x] 4.3 Verify OpenSpec status for `server-web-feature-policy`.
