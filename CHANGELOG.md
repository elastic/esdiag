# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Historical entries before this file existed were reconstructed from the
published release notes, maintenance branches, and tagged history.

## [Unreleased]

### Added

- Added role- and deployment-based onboarding guides for collection and sharing, processing and analysis, local and remote diagnostic clusters, Agent Skills, and shared services.
- Added interactive `esdiag init` onboarding for securely configuring a local diagnostic user, output deployment, collect hosts, and default saved job (#377).
- Added `esdiag agent ask` for finite Kibana Agent Builder questions with explicit conversation follow-ups and Kibana recovery links (#379).
- Added `esdiag process --ask` to start an Agent Builder conversation about a newly processed diagnostic with its identifier included automatically (#379).
- Added `esdiag agent skills` to install the running binary's offline, version-matched ESDiag skill for Claude Code, Codex, and OpenCode (#379).
- Added `esdiag local` to provision and manage a local Elastic Stack through a Rust-owned lifecycle, with core as the default and an explicit full-container override.
- Added optional Elastic Upload Service forwarding to `esdiag-lite.sh` for newly collected and existing ZIP archives.
- Added `esdiag-lite.ps1` for version-aware Elasticsearch diagnostic collection on Windows PowerShell.

### Changed

- Changed diagnostic platform fields to serialize stable hyphenated platform keys (#347).
- Changed platform detection to identify Elastic Cloud Hosted bundles from a cluster license issued to `Elastic Cloud`, so API-only hosted bundles no longer report an unknown platform (#347).
- Changed collection and processing source selection to use canonical registry keys and added a maintainer reconciliation utility for upstream support-diagnostics sources (#348).
- Kept manifests and indexed diagnostics compatible across the platform/application split (#354).
- Changed diagnostic outcome derivation so optional sources absent from imported bundles do not make otherwise successful processing partial (#350).
- Changed `process` to return a non-zero exit when the derived diagnostic outcome is failed (#350).
- Changed synchronous API results to include a derived `outcome` field and align failed statuses with failed report outcomes (#350).
- Changed service-mode web authentication, event delivery, and job admission to use a pluggable auth provider, owner-scoped UI events, and service job caps (#351).
- Changed saved jobs to rewrite legacy `jobs.yml` definitions into the versioned phase-based schema on first read (#353).
- Scoped live `Collect` to Elasticsearch, Kibana, and Logstash; Agent and platform diagnostics now direct users to `Load`/`read` product-provided bundles (#355).
- Changed CLI, web, and synchronous API diagnostics to use one staged execution workflow, including independent processed-document export and raw-bundle upload targets.
- Changed Agent Builder commands and `process --ask` to require the Cargo `agent` feature; the default build continues to include them (#379).
- Replaced `min-diag.sh` with the collection-only, version-aware `esdiag-lite.sh`, using environment-based Elasticsearch authentication, optional ZIP output, and no `jq` runtime dependency.
- Changed Agent Builder progress updates to identify the selected agent by name instead of the generic `Agent Builder` label (#379).
- Changed saved hosts to distinguish target applications from Cloud routing and unresolved URL templates, with clearer validation for legacy host records (#366).
- Changed finite CLI commands to emit typed YAML outcomes on stdout by default, with `--format json` available for interoperability; command failures now return safe structured results and document streams retain their NDJSON-only stdout contract.
- Changed the portable Agent Skill to compose native ESDiag commands and output-deployment configuration, replacing external helper scripts and analysis-specific environment variables (#379).
- Changed omitted CLI output and diagnostic-user resolution to use saved non-secret application preferences after explicit command and environment configuration (#377).
- Changed `esdiag init` to configure only the collection, processing, and asset-installation stages selected by the user.
- Changed `esdiag init` to offer a binary-owned core local stack when local
  processing has no existing deployment.
- Changed `esdiag init` to defer opening a newly created local-stack web UI until every onboarding stage completes.
- Changed `esdiag-local` to retain `auto`, `core`, or `full` stack mode per
  deployment. Core mode uses the matching native binary and avoids an ESDiag
  container; full mode preserves the containerized runtime.

### Fixed

- Fixed `esdiag local` launcher execution and structured outcomes for help output and forwarded state directories (#382).
- Fixed compilation of every `server`, `setup`, and `keystore` feature combination, including `--no-default-features` (#347).
- Fixed the file, stream, and directory exporters reporting a fabricated HTTP `200` request status; they now report the reserved `0` that means "no HTTP transport", so a real Elasticsearch response is distinguishable from a local write (#350).
- Fixed legacy `jobs.yml` migration failing the whole file when one saved job selected a source the current registry no longer knows; such a selection now migrates as authored and is reported when that job runs (#353).
- Fixed `diagnostic.application` and `diagnostic.platform` matching nothing in indices created before those fields were renamed; `setup` now installs the mirrored field alias on them, so a dashboard resolves either provenance name across old and new indices (#354).
- Fixed four ESDiag data views matching on a bare `{class}-{subtype}` prefix, which also matched indices ESDiag does not own; they now pin the `-esdiag` stream suffix (#354).
- Fixed a loaded Elastic Agent diagnostic reporting as skipped by design, which read as "ESDiag will never process this"; it now reports as not yet implemented (#355).

### Security

- Clarified credential custody so saved credentials are mediated by the user-mode keystore, service-mode outputs use runtime-provided credentials, and ad-hoc input API keys remain transient (#352).
- Wrapped every API key, password, and cached keystore password in a redacting type, so credential material renders as a marker in debug and log output and can only be serialized where a field opts in (#352).

## [0.16.0] - 2026-07-11

### Added

- Added configurable `LOG_LEVEL` support to `esdiag-local`.
- Added service-scoped `esdiag-local restart` support for Elasticsearch, Kibana, and ESDiag.
- Added a standalone `esdiag-local` release artifact with generated local-stack state, secure lifecycle and secret commands, and checksum-verified self-updates (#359).
- Added a comprehensive LLM setup guide to document AI assistant resource configurations (#361).
- Added web, CLI, and synchronous API reporting for included diagnostics in ECK and KubernetesPlatform bundles (#336).

### Changed

- Changed standalone web processing so the `Default` output uses `ESDIAG_OUTPUT_*`, keeps collection failures visible in the job feed, fails instead of silently streaming documents to stdout when no output is configured, and generates browser-reachable localhost Kibana links.
- Persisted standalone ESDiag User-mode settings, saved jobs, hosts, and keystore state in a dedicated named volume across container recreation.
- Require Elastic security for `esdiag-local` deployments and remove the unsupported `--insecure` option (#359).
- Changed `esdiag-control` lifecycle commands to use the shared standalone implementation while retaining repository source-build workflows (#359).
- Report the actual setup failure status and output when `esdiag-control` cannot configure the local stack (#359).
- Updated Kibana asset handling to support JSON5 resources with human-readable filenames.
- Updated the local Elastic Stack default to version 9.4.2 to support agent skills (#359).
- Changed Kibana collection and setup to use the shared `kibana-sync` client and bundled asset layout (#341).
- Made embedded documentation minimally Open Knowledge Format compliant while preserving clean docs viewer rendering (#345).
- Changed documentation viewer tag filtering to expose developer-only docs when debug logging is enabled (#345).

### Fixed

- Fixed large Elasticsearch task exports and node-derived metrics ingestion so cluster sends are batched, child streams use their matching templates, and failed sends are recorded in diagnostic reports (#338).
- Fixed Logstash diagnostic processing across fixture versions and allowed Logstash sub-streams to ingest with their concrete data stream datasets (#338).
- Fixed Service Link curl parsing to remove single, double, and escaped quotes from pasted URLs and values (#326).

## [0.15.0] - 2026-06-04

### Added

- Added Tauri-based desktop app support (#253).
- Added multi-platform desktop build workflows (#268).
- Added refined desktop packaging and build workflows (#283).
- Added a built-in documentation viewer (#258).
- Added the Borealis theme system (#258).
- Added a shared `/events` stream for the web interface (#267).
- Added streaming snapshot processing and aligned snapshot data streams (#264).
- Added service and user runtime modes for web interfaces (#266).
- Added host secrets and role targeting (#269).
- Added Logstash API collection support (#274).
- Added Kibana API collection support (#275).
- Added an Advanced page for staged diagnostic workflows (#295).
- Added a Job Builder page for collection and processing (#295).
- Added saved jobs so named configurations can be persisted, listed, and re-run (#300, #317).
- Added collect-to-upload handoff support (#306).
- Added a file-based keystore unlock lease workflow shared by the CLI and web UI (#299, #306).
- Added runtime web feature gating with `--web-features` and `ESDIAG_WEB_FEATURES`.

### Changed

- Increased the long-running collection request timeout so large Elasticsearch API payloads can finish returning (#327).
- Changed diagnostic manifests to record `requested_apis` including status, response time, and response size (#322).
- Moved the Tauri desktop app root under `desktop/` while keeping root-level `cargo tauri build` and desktop packaging workflows working.
- Refined workflow card controls (#295).
- Polished workflow bundle delivery (#295).
- Refined Advanced navigation and Job Builder UI (#295).
- Renamed the web workflow route to `/advanced` and defaulted Job Builder web UI behind the `job-builder` feature (#295).
- Finalized explicit host lifecycle commands (#297, #306).
- Improved saved host authentication persistence (#306).
- Improved agentic CLI summaries and viewer-aware Kibana links (#306).

### Fixed

- Fixed Elasticsearch node stats processing to preserve lookup-enriched node identity fields.
- Fixed Elastic Cloud and GovCloud host normalization to use the documented `_main` single-resource reference (#328).

## [0.14] - 2026-02-25

### Added

- Added failure store enrichment with data stream metadata (#239).
- Added diagnostic `parent_id` propagation (#240).
- Added diagnostic orchestration metadata propagation (#240).
- Added parsed status recording for lookups (#241).
- Added lookup failure recording for lookups (#241).
- Added mapping summaries to index statistics (#243).
- Added support for skipping asset import when security is disabled (#244).
- Added streaming deserialization for large Elasticsearch diagnostics (#247).

### Changed

- Packaged embedded assets as a compressed `assets.tar.gz` archive (#245).
- Auto-generated `NOTICE.txt` in the build script (#249).
- Auto-generated an SBOM in the build script (#249).
- Optimized Raw JSON handling to reduce memory pressure during processing (#251).
- Optimized metadata pre-serialization to reduce processing overhead (#251).
- General maintenance updates refreshed dependencies and build-time tooling.

## [0.13] - 2026-02-03

### Added

- Added Kibana assets to the `setup` command (#208).
- Added Kibana setup support (#217).
- Added cluster metadata to the standardized report documents.
- Added explicit report identifier options to `process`.
- Added filename-based identifiers to `process`.
- Added issue templates.
- Added generated password output during local launch.
- Added `wait_for_completion=true` support for the API key endpoint (#225).

### Changed

- Improved imported diagnostic mappings (#223).
- Improved mapping compatibility (#219).
- Updated dashboard IDs to human-readable values (#228).
- Updated dashboard assets for imported diagnostics.
- Updated local stack defaults.
- Updated bundled Elastic Stack assets.
- Refined API key handling to support `wait_for_completion` in the server flow
  (#225).

### Fixed

- Fixed image tag environment variable handling (#209).
- Fixed host checks on host URLs (#211).
- Fixed Kibana links to use the diagnostic collection date (#212).
- Fixed transport action handling for Elasticsearch diagnostics prior to `8.0`
  (#213).
- Fixed `diagnostic.version` to use the collector version rather than the stack
  version (#214).
- Fixed missing mappings for the `diagnostic.id` field on new installations
  (#218).

## [0.12] - 2025-09-29

### Added

- Added dedicated `setup` feature support in the application (#104).
- Added dedicated `server` feature support in the application (#207).
- Added exporter statistics tracking (#207).
- Added URL-encoded diagnostic identifiers for Kibana links (#207).

### Changed

- Refactored jobs, processors, exporters, and async batch/summary channels to
  simplify the processing pipeline (#170).
- Changed the default web server port to `2501` (#207).
- Hardened local environment startup to reject root execution and improve Linux
  resource detection (#203).
- Simplified local image naming for container-based development (#204).

### Fixed

- Fixed missing warnings when running local startup as the `root` user (#205).
- Fixed Kibana filter values to use URL encoding (#206).

## [0.11] - 2025-09-17

### Added

- Added secure local environment bootstrap.
- Added health checks for the local environment.
- Added early setup validation for the local environment.
- Added Cloud API support (#175).
- Added configurable output connection limits.
- Added role-aware assets (#197).
- Added sample diagnostic bundles for testing (#199).
- Added initial index statistics test coverage (#199).

### Changed

- Renamed local environment commands from `launch` and `remove` to `up` and
  `down`.
- Reworked the processing and export pipeline around async channels, lazy task
  spawning, and dedicated workers for heavier processors.
- Updated Datastar and crate dependencies, then reverted the UI loading mode to
  restore the preferred behavior on the branch.

### Fixed

- Fixed shard statistics enrichment (#199).

## [0.10.1] - 2025-08-25

### Added

- Added the `bin/esdiag-control` script to bootstrap and manage a local
  container-based development environment (#190).

### Changed

- Refined the API key and service link server APIs (#186).
- Applied minor UX polish, documentation updates, and test cleanup across the
  early web workflow (#186).

### Fixed

- Fixed pasted diagnostic URLs with trailing periods so archive uploads do not
  fail on invalid zip paths (#177).
- Fixed ECK diagnostic path handling for correctly structured archives (#179).

[Unreleased]: https://github.com/elastic/esdiag/compare/0.16.0...main
[0.16.0]: https://github.com/elastic/esdiag/compare/0.15.0...0.16.0
[0.15.0]: https://github.com/elastic/esdiag/compare/0.14.2...0.15.0
[0.14]: https://github.com/elastic/esdiag/compare/0.13.0...0.14.2
[0.13]: https://github.com/elastic/esdiag/compare/0.12.0...0.13.0
[0.12]: https://github.com/elastic/esdiag/compare/0.11.1...0.12.0
[0.11]: https://github.com/elastic/esdiag/compare/0.10.2...0.11.1
[0.10.1]: https://github.com/elastic/esdiag/compare/0.10.0...0.10.2
