## 1. Portable Package Foundation

- [x] 1.1 Add canonical `.agents/skills/esdiag/` instructions and references plus deterministic synchronization into the distributable package.
- [x] 1.2 Add valid thin Claude Code and Codex manifests and document direct OpenCode discovery.
- [x] 1.3 Remove separate Claude-only workflow copies and assert generated package content cannot drift from the canonical skill.
- [x] 1.4 Verify the packaged skill installs without a source checkout, container runtime, or reachable Elastic deployment.

## 2. Saved Job Terminal Facts

- [x] 2.1 Return the unified executor's composite `JobOutcome` from saved-job execution so retained save, process, and send results can coexist.
- [x] 2.2 Report every durable diagnostic identifier and Kibana link, retained archive path, and upload destination produced by a phase-composed job without treating them as mutually exclusive variants.
- [x] 2.3 Add unit and CLI coverage for every job outcome without exposing credentials.

## 3. Agent Builder Validation Findings

- [x] 3.1 Verify that Agent Builder MCP exposes tools rather than an agent/skill invocation surface and that workflow `agent-id` is not runtime-templatable.
- [x] 3.2 Verify asynchronous Agent Builder chat progress events, unstructured completed messages, relative dashboard links, and interruption behavior against recorded fixtures.
- [x] 3.3 Verify caller privilege requirements for Agent Builder and ESDiag data streams with a minimally scoped credential.
- [x] 3.4 Measure cluster inference cost and document that model availability is proven only by a real agent request.
- [x] 3.5 Preserve the requirement that every real analysis and follow-up lands in Kibana Agent Builder conversation history.

## 4. Successor CLI Boundaries

- [ ] 4.1 Complete `standardize-cli-output` so the skill consumes exact YAML/JSON outcomes and no longer parses completion prose.
- [ ] 4.2 Complete `add-first-run-onboarding` so the plugin owns no URLs, credentials, saved-job defaults, or prompt-driven first-job setup and routes new users through `references/onboarding.md`.
- [ ] 4.3 Complete `add-agent-cli` so the skill delegates through `esdiag agent ask`, uses canonical output deployment configuration, persists no local conversation map, runs no direct freshness query, and can be installed from the running binary through `esdiag agent skills`.
- [ ] 4.4 Remove canonical and generated skill script directories and regenerate identical Claude Code, Codex, and OpenCode packages.

## 5. Documentation And Archive Verification

- [ ] 5.1 Update installation and skill documentation to distinguish normal use, local `esdiag init` onboarding, and cluster provisioning without exposing credentials to agent conversations.
- [ ] 5.2 Document Agent Builder inference cost, required privileges, explicit conversation continuation, Kibana handoff, and unsafe retry behavior.
- [ ] 5.3 Update `CHANGELOG.md` using the repository changelog skill for the final portable script-free workflow.
- [ ] 5.4 Run plugin/package tests, `cargo clippy`, `cargo test`, and strict validation for this change and all three successor changes before archive.
