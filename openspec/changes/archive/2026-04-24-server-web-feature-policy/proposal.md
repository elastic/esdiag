## Why

The web UI currently uses runtime mode as the coarse switch for several user-facing pages, which makes it hard to hold back individual UI surfaces that are not release-ready. Job Builder should remain available through saved-job CLI support, but its web page and browser-facing routes need runtime release control independent of Rust compile-time features.

## What Changes

- Introduce a unified `ServerPolicy` that composes runtime mode and explicit web feature availability.
- Add `ESDIAG_WEB_FEATURES` and `serve --web-features` as comma-separated allowlists for optional web UI features, starting with `advanced` and `job-builder`.
- Make `ESDIAG_WEB_FEATURES` authoritative when set: unset uses runtime-mode defaults, set uses exactly the listed features, and an empty value disables optional web features.
- Keep service-mode safety constraints in force; web feature flags do not bypass service-mode authentication, persistence, or local-artifact restrictions.
- Rename the Advanced page route from `/workflow` to `/advanced`, update navigation to use the Advanced page title, and remove `/workflow` without redirect because the route is unreleased.
- Gate the Job Builder page and saved-job web routes behind the `job-builder` web feature while preserving CLI saved-job commands and persistence.

## Capabilities

### New Capabilities

- `web-feature-policy`: Defines runtime web feature allowlist parsing, defaults, and policy composition with server runtime mode.

### Modified Capabilities

- `web-runtime-modes`: Replaces direct runtime-mode page exposure rules with `ServerPolicy` decisions and renames `/workflow` to `/advanced`.
- `saved-jobs`: Clarifies that saved jobs remain available to the CLI while Job Builder web UI and saved-job web routes are optional web features.

## Impact

- Affects Web UI routing, navigation templates, server CLI arguments, server startup policy construction, and tests for route availability.
- Affects documentation and changelog because the user-facing Advanced route changes and Job Builder web UI becomes feature-gated.
- Does not change Elastic product processing logic, diagnostic collection, exporters, saved-job file format, or CLI saved-job behavior.
