---
type: Maintainer Guide
title: Repository Organization
description: Maintainer guide to the repository layout and major source directories.
tags: [repository, maintainers]
---

Repository Organization
=======================

This document explains the high-level layout of the repository so contributors can quickly find the right area before making changes.

Top-Level Layout
----------------

```text
.
├── .agents/
├── .github/
├── .gitignore
├── assets/
├── bin/
├── crates/
├── desktop/
├── docker/
├── docs/
├── openspec/
├── plugin/
├── src/
├── templates/
└── tests/
```

- `.agents/`: Shared agent skills for the repository.
- `.github/`: GitHub automation such as Actions workflows and issue templates.
- `.gitignore`: Root ignore rules for generated files, local artifacts, and other untracked content that should not be committed.
- `assets/`: Elastic Stack assets installed into target environments, such as configuration and setup content used by `esdiag setup`.
- `bin/`: User-facing helpers including standalone `esdiag-local`, repository build wrapper `esdiag-control`, and `esdiag-lite.sh`.
- `crates/`: Independently packaged workspace libraries. `crates/elasticrc`
  provides Apache-2.0-licensed, read-only Elastic CLI context resolution.
- `desktop/`: Tauri desktop app root, including desktop config, capabilities, icons, packaging assets, and desktop-only build scripts.
- `gen/`: Generated Tauri schema output at the repo root during desktop builds; this directory is build output and is not tracked.
- `docker/`: Container and Compose definitions for local and packaging-related workflows.
- `docs/`: User-facing and maintainer-facing documentation.
- `openspec/`: OpenSpec change proposals, archived changes, and repository specification artifacts.
- `plugin/`: Distributable script-free Agent Skill package with thin Claude Code and Codex manifests. Its bundled `SKILL.md` and `references/` under `plugin/skills/` are generated from `.agents/skills/esdiag/` by `bin/sync-plugin-skill.sh` and must not be edited directly. The repository-root `.claude-plugin/marketplace.json` publishes the Claude package; Codex and OpenCode discover the canonical `.agents/skills/` copy in a checkout, while `esdiag agent skills` installs the matching embedded skill for binary users.
- `src/`: Rust application and library source code.
- `templates/`: Askama HTML templates that power the web UI.
- `tests/`: Integration-style tests, CLI tests, runtime tests, and test fixtures.

`src/` First-Level Layout
-------------------------

The Rust code is split into a small set of first-level modules plus a few root files:

```text
src/
├── client/
├── data/
├── exporter/
├── processor/
├── receiver/
├── server/
├── embeds.rs
├── env.rs
├── job.rs
├── lib.rs
├── main.rs
├── onboarding.rs
├── setup.rs
└── elastic_upload_service.rs
```

### Directories

- `src/client/`: HTTP client implementations for Elastic Stack products such as Elasticsearch, Kibana, and Logstash.
- `src/data/`: Shared domain types, configuration models, known host handling, keystore support, settings, and workflow data structures.
- `src/exporter/`: Output adapters that write processed data to Elasticsearch, files, directories, archives, or stdout.
- `src/processor/`: Diagnostic collection and transformation pipeline that turns raw inputs into normalized reports and exported documents.
- `src/receiver/`: Input adapters that read diagnostics from local archives,
  directories, remote services, and Elastic Upload Service links.
- `src/server/`: Axum-based HTTP server and web UI runtime for archive
  submissions, settings, docs, Advanced page, and related browser-facing
  features.

### Root Files

- `src/lib.rs`: Library module declarations and shared exports used by the binary and tests.
- `src/main.rs`: Main CLI entrypoint, command definitions, and runtime orchestration.
- `src/setup.rs`: Asset installation logic for Elasticsearch and Kibana setup flows.
- `src/elastic_upload_service.rs`: Elastic Upload Service adapter used by the
  generic Send stage.
- `src/job.rs`: Saved job execution and management helpers.
- `src/onboarding.rs`: Flow-neutral first-run configuration, persistence, and readiness operations shared by terminal and future GUI flows.
- `src/env.rs`: Environment-variable defaults and lookup helpers.
- `src/embeds.rs`: Embedded static asset wiring used by the application.
