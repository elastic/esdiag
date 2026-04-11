# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Historical entries before this file existed were reconstructed from the
published release notes, maintenance branches, and tagged history.

## [Unreleased]

### Added

- Added Tauri-based desktop app support (#253).
- Added multi-platform desktop build workflows (#268).
- Added refined desktop packaging and build workflows (#283).
- Added a built-in documentation viewer (#258).
- Added the Borealis theme system (#258).
- Added a shared `/events` stream for the web interface.
- Added streaming snapshot processing and aligned snapshot data streams.
- Added service and user runtime modes for web interfaces.
- Added host secrets and role targeting on the upstream branch.
- Added Logstash API collection support (#274).
- Added Kibana API collection support (#275).
- Added a staged diagnostic workflow.
- Added a jobs workflow for collection and processing.
- Added saved jobs so named configurations can be persisted, listed, and re-run.
- Added collect-to-upload handoff support.
- Added a file-based keystore unlock lease workflow shared by the CLI and web UI.

### Changed

- Refined workflow card controls.
- Polished workflow bundle delivery.
- Refined workflow navigation and jobs UI.
- Finalized explicit host lifecycle commands.
- Improved saved host authentication persistence.
- Improved agentic CLI summaries and viewer-aware Kibana links.

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

[Unreleased]: https://github.com/elastic/esdiag/compare/0.14.2...preview
[0.14]: https://github.com/elastic/esdiag/compare/0.13.0...0.14.2
[0.13]: https://github.com/elastic/esdiag/compare/0.12.0...0.13.0
[0.12]: https://github.com/elastic/esdiag/compare/0.11.1...0.12.0
[0.11]: https://github.com/elastic/esdiag/compare/0.10.2...0.11.1
[0.10.1]: https://github.com/elastic/esdiag/compare/0.10.0...0.10.2
