## 1. Agent Builder Client Boundary

- [x] 1.1 Add focused Agent Builder request, SSE event, completion, usage, and safe failure types that compose an existing configured `KibanaClient`.
- [x] 1.2 Extend `KibanaClient` only as needed for incremental response-body consumption while retaining its existing URL, authentication, TLS, and Kibana header behavior.
- [x] 1.3 Add recorded response fixtures for conversation IDs, reasoning, tool events, completion, relative links, HTTP errors, unknown events, and interrupted streams.

## 2. Embedded Skill Installer

- [x] 2.1 Add a `RustEmbed` asset set for canonical ESDiag `SKILL.md`, `references/`, and supported `agents/` metadata that excludes scripts and exposes a deterministic manifest and digest.
- [x] 2.2 Add Claude Code, Codex, and OpenCode user-scope target adapters with documented home overrides, positive detection, explicit selection, and fixture-backed path tests.
- [x] 2.3 Implement read-only preflight that classifies missing, exact, intact installer-owned, modified, and unrecognized skill directories before any mutation.
- [x] 2.4 Implement sibling staging, validation, recoverable backup, atomic replacement where supported, ownership marker written last, and explicit-force conflict handling.
- [x] 2.5 Add `esdiag agent skills` with automatic multi-target detection, repeatable `--target` selection, guarded `--force`, structured per-target actions, partial-failure context, and restart/reload guidance.
- [x] 2.6 Verify the canonical skill is included by `cargo package`, embedded in feature combinations that expose the CLI, installable without network access, and byte-equivalent across supported targets.
- [x] 2.7 Compose the embedded installer in process for `esdiag init`, present detected and explicit target choices, keep decline or installation failure non-gating, and include per-target results plus standalone recovery guidance.

## 3. Agent Ask Command

- [x] 3.1 Add `esdiag agent ask <PROMPT>` with `--agent`, `--conversation`, and mutually exclusive `--new` options and default agent `elastic-ai-agent`.
- [x] 3.2 Resolve the Kibana viewer, space, and authentication exclusively through the canonical output deployment and reject missing or incomplete viewer configuration.
- [x] 3.3 Submit the prompt to the space-scoped Agent Builder async conversation endpoint without adding inference routing or diagnostic-specific fields.
- [x] 3.4 Render reasoning and tool progress on stderr and return one finite structured agent-response outcome containing the completed message, conversation ID, Kibana link, and safe optional usage metadata.
- [x] 3.5 Resolve relative Kibana markdown links against the configured viewer and add exact YAML/JSON outcome tests.
- [x] 3.6 Implement explicit conversation continuation without local conversation persistence and prove new asks never consult or create a local mapping file.
- [x] 3.7 Return categorized non-zero structured failures, including `retry_safe: false` plus conversation recovery data after interrupted conversation creation, without automatic retry.
- [x] 3.8 Add `process --ask <PROMPT>` to submit a new Agent Builder question about the successfully processed diagnostic, including exact prompt construction and CLI tests.
- [x] 3.9 Prefix Agent Builder stderr progress with the selected agent's display name, using a non-blocking ID-derived fallback when its name cannot be read.
- [x] 3.10 Gate `agent ask`, `agent skills`, and `process --ask` behind the Cargo `agent` feature while retaining them in the default `full` build.

## 4. Portable Skill Simplification

- [x] 4.1 Update the canonical skill to call `esdiag agent ask`, pass exact diagnostic IDs from structured outcomes when available, and delegate discovery of existing diagnostics to Agent Builder.
- [x] 4.2 Remove analysis-specific URL, credential, inference, saved-job, freshness, and local-conversation configuration from skill instructions and references.
- [x] 4.3 Delete `.agents/skills/esdiag/scripts/` and `plugin/skills/esdiag/scripts/`, remove scripts from the sync contract, and assert both directories remain absent from source, generated, and embedded assets.
- [x] 4.4 Replace shell-helper fixtures with native command and recorded SSE tests, then regenerate the portable skill for Claude Code, Codex, OpenCode, and embedded installation.

## 5. Documentation And Compatibility

- [x] 5.1 Document `esdiag agent ask`, explicit follow-up, Kibana history handoff, output-deployment prerequisites, inference cost, and interrupted-request recovery.
- [x] 5.2 Document `esdiag agent skills`, automatic detection, explicit targets, safe updates/conflicts, user scope, offline/version-correct behavior, and agent reload expectations.
- [x] 5.3 Document that `agent converse`, public SSE/NDJSON proxying, binding, freshness lookup, and local conversation persistence are unsupported and out of scope.
- [x] 5.4 Update `CHANGELOG.md` using the repository changelog skill and remove obsolete plugin environment-variable guidance.

## 6. Verification

- [x] 6.1 Run formatting and targeted Kibana client, Agent Builder SSE, embedded asset, installer safety, CLI outcome, redaction, interruption, and portable-skill tests.
- [x] 6.2 Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] 6.3 Run `cargo test --all-features`.
- [x] 6.4 Verify no `converse` command or public agent event stream exists and no installed skill references external executable helpers.
- [x] 6.5 Run strict OpenSpec validation for `add-agent-cli`.
