## Context

ESDiag currently has a local `crate::client::KibanaClient` that wraps `reqwest` with Kibana authentication, `kbn-xsrf`, generic request dispatch, multipart upload support, and `/api/status` connection testing. `KibanaReceiver` builds on that client to cache the Kibana version, discover spaces, collect raw responses with timing and byte metrics, and expose the shared `Receive` / `ReceiveRaw` traits used by the processor.

The published `kibana-sync` crate now owns the same low-level Kibana client concerns and adds reusable version parsing, capability checks, space-scoped clients, request concurrency limits, and saved-object-oriented helpers. ESDiag should consume that crate without changing the user-visible Kibana collection contract already defined in `kibana-diagnostic-collection`.

## Goals / Non-Goals

**Goals:**

- Make `kibana-sync` the canonical implementation for Kibana HTTP client construction and request dispatch.
- Keep ESDiag's `Receiver` trait boundary stable so collection orchestration and processor type-state flows do not need structural changes.
- Preserve Kibana collection output, manifest metadata, request retry behavior, response timing, response size accounting, and error status/body reporting.
- Preserve saved host conversion and existing CLI/Web UI behavior for Kibana connection tests and collection jobs.
- Establish the base client dependency that a follow-up bundled Kibana assets change can use for `kibana-sync` saved object, space, agent, tool, and workflow support.

**Non-Goals:**

- Rewriting source catalog resolution, diagnostic type selection, pagination planning, or archive layout.
- Expanding bundled Kibana assets in this change; that work is explicitly planned as a separate follow-up.
- Adding new CLI flags, Web UI controls, or user-facing Kibana sync workflows.
- Replacing Elasticsearch, Logstash, Elastic Cloud, or upload clients.
- Depending on external binaries or runtime services beyond the existing Kibana target.

## Decisions

### 1. Keep ESDiag's receiver traits and adapt the client underneath

Replace the local thin Kibana client implementation with a small compatibility layer around `kibana_sync::KibanaClient`, or re-export the crate type directly where call sites can use it cleanly. `KibanaReceiver` remains the owner of ESDiag-specific behavior: manifest construction, raw response conversion, metrics capture, lazy spaces discovery, and trait implementations.

**Rationale:** `kibana-sync` should own HTTP mechanics, while ESDiag's receiver still owns diagnostic semantics. This keeps the blast radius limited to Kibana-specific modules and avoids changing processor lifecycle state transitions.

**Alternative considered:** Move collection logic directly onto `kibana-sync` extractors. That is premature because ESDiag's collection is driven by `assets/kibana/sources.yml`, not by a fixed saved-object/agent/tool bundle model.

### 2. Map host authentication explicitly

Add a conversion from `crate::data::Auth` to `kibana_sync::Auth`:

- `Auth::Basic(username, password)` -> `kibana_sync::Auth::Basic(username, password)`
- `Auth::Apikey(key)` -> `kibana_sync::Auth::Apikey(key)`
- `Auth::None` -> `kibana_sync::Auth::None`

Build clients via `kibana_sync::KibanaClient::builder(url).auth(mapped_auth).max_concurrency(current_limit).build()`, where `current_limit` preserves ESDiag's existing Kibana request concurrency.

**Rationale:** The mapping is exact for Basic and API key authentication. For no-auth targets, `kibana-sync` correctly omits the `Authorization` header instead of sending the local client's previous `Authorization: None` value.

**Alternative considered:** Keep the old local header construction. That would preserve accidental behavior but undermine the goal of using the shared crate as the canonical client implementation.

### 3. Keep ESDiag's source-driven space expansion during this refactor

Continue discovering spaces through the receiver and continue letting `KibanaCollector` expand `spaceaware: true` source entries. When a source path is manually prefixed with `/s/{space}`, send it through a root `kibana-sync` client so paths are not double-prefixed.

**Rationale:** `kibana-sync` supports space-scoped clients from a registry, but ESDiag discovers spaces lazily from the target Kibana instance. Preserving the current expansion model minimizes behavior changes and keeps archive paths stable.

**Alternative considered:** Rebuild the client with a populated `SpaceRegistry` after `get_spaces()` and use `client.space(id)`. This can be revisited later, but it creates more room for path-prefix and default-space regressions during a client replacement.

### 4. Preserve ESDiag's request outcome model

Keep `KibanaRequestError` as the diagnostic-facing error type for non-success HTTP responses. `KibanaReceiver` should still measure elapsed time, read the response body, calculate byte size, and convert non-2xx responses into `KibanaRequestError` so retry and manifest metrics remain stable.

Transport failures from `kibana-sync` should be mapped so `should_retry_kibana_error()` treats `kibana_sync::Error::Transport(reqwest::Error)` the same way it currently treats a direct `reqwest::Error`.

**Rationale:** `kibana-sync` returns `reqwest::Response` for raw requests, which lets ESDiag keep its HTTP status/body handling. The only meaningful adapter work is error downcasting for retries.

**Alternative considered:** Use `kibana_sync::Error::ApiResponse` for all non-success handling. That loses ESDiag's response timing and size accounting unless wrapped again, so it does not simplify the collection path.

### 5. Pin the dependency first, then expand bundled Kibana assets separately

Add `kibana-sync = "0.1.0"` to `Cargo.toml` and migrate only the thin-client responsibilities in this change.

**Rationale:** The crate was just published, and the immediate goal is reducing duplicated client code. ESDiag will adopt the higher-level saved object, space, agent, tool, and workflow APIs for bundled Kibana assets in a follow-up change after the base dependency is proven in ESDiag's compatibility tests.

**Alternative considered:** Immediately replace ESDiag's saved-object asset setup and all future Kibana workflows with `kibana-sync` modules. That would mix a client refactor with product behavior changes.

### 6. Preserve ESDiag's existing Kibana concurrency limit

Keep ESDiag's current Kibana collection concurrency behavior and pass the same limit into `kibana-sync` through `KibanaClientBuilder::max_concurrency`. The existing collector executes API fetches with a bounded `buffer_unordered(5)` pattern, so the shared client should be configured to the same effective request ceiling unless a future change introduces a first-class concurrency setting.

**Rationale:** The client migration should not silently increase or decrease request pressure on Kibana. Passing ESDiag's current limit into the crate keeps the crate's semaphore aligned with the collector's scheduler.

**Alternative considered:** Use the `kibana-sync` default concurrency. That would couple ESDiag behavior to a dependency default and could change request pressure without an ESDiag code change.

## Risks / Trade-offs

- **[Risk] No-auth behavior changes from `Authorization: None` to no authorization header.** -> Mitigation: add/adjust tests around `Auth::None` construction and verify connection tests still work against unsecured Kibana.
- **[Risk] Space-aware paths could be double-prefixed if root and scoped clients are mixed.** -> Mitigation: keep source-driven prefixing in one place and add unit coverage for generated request paths.
- **[Risk] Retry classification may stop recognizing transport failures because the concrete error type changes.** -> Mitigation: update retry helpers to inspect `kibana_sync::Error::Transport` and retain existing `reqwest::Error` handling.
- **[Risk] Dependency version drift could introduce Kibana behavior changes unexpectedly.** -> Mitigation: pin the initial dependency to the published compatible version and rely on normal Cargo lockfile review for upgrades.
- **[Risk] Two semver/version helpers may coexist during migration.** -> Mitigation: prefer `kibana_sync::parse_kibana_version` or `server_version()` in Kibana code and remove redundant local status parsing where it no longer adds diagnostic value.
- **[Risk] `kibana-sync`'s default concurrency could change effective collection pressure.** -> Mitigation: pass ESDiag's existing Kibana request concurrency limit into the shared client builder.

## Migration Plan

1. Add the `kibana-sync` dependency and run a compile check to reveal call-site changes.
2. Replace or adapt `src/client/kibana.rs` so existing `crate::client::KibanaClient` users are backed by `kibana_sync::KibanaClient`.
3. Update `KibanaReceiver` to use `kibana-sync` for connection tests, raw requests, and version parsing while retaining ESDiag's metrics/error wrapper.
4. Update retry helpers in `src/processor/kibana/collector.rs` for the new transport error shape.
5. Run focused Kibana receiver/client tests, then broader `cargo test` or `cargo check` as time allows.
6. If the migration causes regressions, rollback is limited to removing the dependency and restoring the previous `src/client/kibana.rs` implementation.

## Follow-up Direction

- Create a separate OpenSpec change to expand ESDiag's bundled Kibana assets across the higher-level resource types supported by `kibana-sync`: saved objects, spaces, agents, tools, and workflows.
