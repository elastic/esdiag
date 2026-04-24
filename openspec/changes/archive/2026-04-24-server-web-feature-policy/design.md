## Context

The server currently stores `RuntimeMode` and `RuntimeModePolicy` in `ServerState`, and route registration consults runtime mode directly to decide whether user-mode pages such as `/workflow` and `/jobs` are mounted. This made sense when page availability mapped cleanly to `user` versus `service`, but Job Builder now needs to remain implemented for CLI saved jobs while being withheld from the web release surface.

The Advanced workflow page has the same shape as future optional web surfaces: it is a browser feature, not a Rust compile-time feature. The policy layer needs to represent both operating mode constraints and explicit web feature exposure.

## Goals / Non-Goals

**Goals:**

- Replace `RuntimeModePolicy` with a unified `ServerPolicy` that owns server capability decisions.
- Add a typed `WebFeature` allowlist parsed from `serve --web-features` or `ESDIAG_WEB_FEATURES`.
- Make `advanced` and `job-builder` independent web features while keeping service-mode restrictions authoritative.
- Rename the Advanced page URL from `/workflow` to `/advanced` and rename workflow terminology to Advanced where it refers to the web page.
- Keep saved-job CLI behavior and `jobs.yml` persistence unchanged.

**Non-Goals:**

- No Rust compile-time feature gate for Job Builder.
- No changes to saved-job YAML format or CLI job subcommands.
- No service-mode enablement for local-artifact-backed pages.
- No remote or dynamic feature flag service.

## Decisions

### Use one `ServerPolicy`

`ServerPolicy` will contain the runtime mode and parsed web feature set:

```rust
pub struct ServerPolicy {
    runtime_mode: RuntimeMode,
    web_features: WebFeatureSet,
}
```

Handlers, route registration, and templates should ask business-level questions such as `allows_advanced()`, `allows_job_builder()`, `allows_host_management()`, and `requires_iap_headers()`. This avoids scattering combined checks like `runtime_mode == User && web_features.contains(...)`.

Alternative considered: keep separate `RuntimeModePolicy` and `WebFeaturePolicy`. That keeps inputs pure, but creates repeated composition at every call site. A unified policy keeps the input separation internally while presenting one decision API.

### Make web feature overrides authoritative when set

`serve --web-features` and `ESDIAG_WEB_FEATURES` will be parsed as comma-separated lists of kebab-case feature names. The CLI argument takes precedence over the environment variable. Initial names:

- `advanced`
- `job-builder`

Behavior:

- Unset: use mode-aware defaults.
- Set to non-empty: enable exactly the listed known features.
- Set to empty or whitespace: disable optional web features.
- Unknown names: fail startup with a clear error that names the invalid value and lists the known values.

Default features:

- `user`: `advanced`
- `service`: none

Desktop startup uses the same user-mode defaults as `serve --mode user` unless an explicit web feature override is supplied through the environment or desktop launch configuration.

This makes Job Builder hidden by default while preserving the Advanced page by default in user mode.

Alternative considered: make the env var additive. Additive flags are convenient for experiments, but they make release exposure harder to reason about because defaults and overrides combine implicitly.

### Keep runtime mode as the safety envelope

Feature flags only expose optional web surfaces inside the current runtime mode's safety constraints. For example, `ESDIAG_WEB_FEATURES=job-builder` in service mode must not mount Job Builder because service mode does not allow local `jobs.yml` reads or writes.

This keeps `ESDIAG_WEB_FEATURES` from becoming an accidental bypass around IAP, exporter, host-management, keystore, or local persistence rules.

### Rename workflow page terminology to Advanced

The Advanced page should be mounted at `/advanced`, and navigation should point to `/advanced`. Internal Rust handler names, template names, CSS identifiers, docs, and user-facing copy that describe the web page as "workflow" should be renamed to Advanced where practical in the same change.

The old `/workflow` route is unreleased and should not have a compatibility redirect. Requests to `/workflow` should return the same not-found behavior as any unmounted route.

### Gate web saved-job routes with Job Builder

The Job Builder feature owns browser-facing saved-job routes:

- `/jobs`
- `/jobs/saved`
- `/jobs/saved/{name}`

The CLI owns saved-job lifecycle commands and must not consult `ServerPolicy`.

## Risks / Trade-offs

- Route rename can break local unreleased bookmarks → No compatibility redirect; update docs and tests to treat `/workflow` as unmounted.
- Authoritative env parsing can surprise users who set only `job-builder` and lose `advanced` → Document that values are exact and recommend `ESDIAG_WEB_FEATURES=advanced,job-builder`.
- Unknown feature startup failure can block deployment after a typo → Emit a clear error naming the invalid token and valid values.
- Template conditionals can drift from route policy → Pass explicit booleans from `ServerPolicy` to templates rather than recomputing from raw mode strings.
