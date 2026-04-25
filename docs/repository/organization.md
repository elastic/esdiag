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
├── desktop/
├── docker/
├── docs/
├── openspec/
├── src/
├── templates/
└── tests/
```

- `.agents/`: Shared agent skills for the repository.
- `.github/`: GitHub automation such as Actions workflows and issue templates.
- `.gitignore`: Root ignore rules for generated files, local artifacts, and other untracked content that should not be committed.
- `assets/`: Elastic Stack assets installed into target environments, such as configuration and setup content used by `esdiag setup`.
- `bin/`: User-facing helper executables such as `esdiag-control` and `min-diag.sh`.
- `desktop/`: Tauri desktop app root, including desktop config, capabilities, icons, packaging assets, and desktop-only build scripts.
- `gen/`: Generated Tauri schema output at the repo root during desktop builds; this directory is build output and is not tracked.
- `docker/`: Container and Compose definitions for local and packaging-related workflows.
- `docs/`: User-facing and maintainer-facing documentation.
- `openspec/`: OpenSpec change proposals, archived changes, and repository specification artifacts.
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
├── setup.rs
└── uploader.rs
```

### Directories

- `src/client/`: HTTP client implementations for Elastic Stack products such as Elasticsearch, Kibana, and Logstash.
- `src/data/`: Shared domain types, configuration models, known host handling, keystore support, settings, and workflow data structures.
- `src/exporter/`: Output adapters that write processed data to Elasticsearch, files, directories, archives, or stdout.
- `src/processor/`: Diagnostic collection and transformation pipeline that turns raw inputs into normalized reports and exported documents.
- `src/receiver/`: Input adapters that read diagnostics from local archives, directories, remote services, and upload links.
- `src/server/`: Axum-based HTTP server and web UI runtime for uploads, settings, docs, Advanced page, and related browser-facing features.

### Root Files

- `src/lib.rs`: Library module declarations and shared exports used by the binary and tests.
- `src/main.rs`: Main CLI entrypoint, command definitions, and runtime orchestration.
- `src/setup.rs`: Asset installation logic for Elasticsearch and Kibana setup flows.
- `src/uploader.rs`: Upload logic for sending collected archives to the Elastic Upload Service.
- `src/job.rs`: Saved job execution and management helpers.
- `src/env.rs`: Environment-variable defaults and lookup helpers.
- `src/embeds.rs`: Embedded static asset wiring used by the application.
