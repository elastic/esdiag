## 1. Standalone Script Foundation

- [ ] 1.1 Add `bin/esdiag-local` with location-independent command parsing, release version constants, runtime selection, dependency checks, and managed state-directory resolution.
- [ ] 1.2 Embed and generate the secure and insecure Compose configurations plus initial environment state without reading repository files.
- [ ] 1.3 Apply restrictive state and credential permissions and add safe loading/updating of generated environment values.
- [ ] 1.4 Implement exact default image references for Elasticsearch, Kibana, and ESDiag with explicit image, registry, version, and pull-policy overrides, including `ESDIAG_IMAGE_TAG` precedence over stored and embedded ESDiag image defaults.
- [ ] 1.5 Target Bash 3.2+ and add context-aware Linux/macOS/WSL adapters for paths, checksums, resource inspection, browser launch, and clipboard tools.
- [ ] 1.6 Replace `.env` sourcing with an allowlisted, non-evaluating parser and atomic restrictive writes.

## 2. Local Stack Lifecycle

- [ ] 2.1 Implement idempotent staged `up`: validate, configure, pull, start Elasticsearch/Kibana, create credentials, run `esdiag setup`, start ESDiag, and verify endpoints.
- [ ] 2.2 Implement bounded readiness waits and failure-log capture for Elasticsearch, Kibana, setup, and ESDiag service failures.
- [ ] 2.3 Implement `down`, `status`, `logs`, `setup`, `auth`, `secrets`, `reset`, `update`, `help`, and `version` with non-destructive defaults and confirmation for destructive reset.
- [ ] 2.4 Bind public ports to loopback, use separate Elasticsearch and Kibana volumes, and preserve secure-by-default behavior with explicit `--insecure` support.
- [ ] 2.5 Add state schema/version tracking and require explicit `up --upgrade` before changing versions in an existing deployment.
- [ ] 2.6 Implement raw-output `secrets password` and `secrets apikey` commands with errors isolated to standard error and no disclosure through status or logs.
- [ ] 2.7 Add platform-aware clipboard detection, help examples, best-effort password copying immediately before browser launch, and the `--copy-password=false` opt-out.
- [ ] 2.8 Add `.env` defaults and validation for `ESDIAG_ELASTICSEARCH_PORT`, `ESDIAG_KIBANA_PORT`, and `ESDIAG_PORT`, including range, uniqueness, and availability checks.
- [ ] 2.9 Persist one shared ESDiag API key for setup and service execution and fail closed on missing or mismatched environment/volume state without automatic recovery.
- [ ] 2.10 Implement transactional `up --upgrade` state changes and print the stack-upgrade next step after successful script self-update.
- [ ] 2.11 Add validated Podman-first/Docker-fallback runtime detection, explicit override support, and provider-specific Compose capability adapters.

## 3. Repository Build Integration

- [ ] 3.1 Retain repository validation plus `build` and `buildx` behavior in `bin/esdiag-control` for source-built and audited container workflows.
- [ ] 3.2 Delegate `esdiag-control up`, `down`, `setup`, and `auth` to `bin/esdiag-local`; for `up` and setup execution, export `ESDIAG_IMAGE_TAG=esdiag:${version}` and disable pulling so the repository-built image is used.
- [ ] 3.3 Remove duplicated lifecycle/template logic from `esdiag-control` after delegation is covered by tests.
- [ ] 3.4 Default delegated `esdiag-control` state to `${repository_root}/target/esdiag-local` while preserving an explicit caller override.

## 4. Release and Documentation

- [ ] 4.1 Add release packaging that renders an executable `esdiag-local` asset with version and image constants pinned to the tagged release.
- [ ] 4.2 Gate script publication on availability of the matching `docker.elastic.co/esdiag/esdiag` image manifests for supported architectures.
- [ ] 4.3 Add a release action that attaches the version-pinned script as `esdiag-local` and its checksum as `esdiag-local.sha256` to each GitHub release.
- [ ] 4.4 Implement `update --check` against official `github.com/elastic/esdiag` releases with semantic version comparison and no local mutation.
- [ ] 4.5 Implement checksum-verified `curl` download, downloaded-script validation, executable permissions, and atomic self-replacement for `update`.
- [ ] 4.6 Add non-destructive handling for current releases, unreachable GitHub, invalid checksums, invalid downloaded scripts, and non-writable installation paths.
- [ ] 4.7 Update the README and local deployment documentation to use `https://ela.st/esdiag-local` for latest-release discovery and distinguish official-image `esdiag-local` usage from source-build `esdiag-control` usage.
- [ ] 4.8 Update nearby repository and command documentation for generated state, prerequisites, lifecycle commands, secret retrieval and clipboard behavior, script updates, stack upgrades, security defaults, and destructive reset behavior.
- [ ] 4.9 Add an Unreleased `CHANGELOG.md` entry describing the standalone artifact, self-update workflow, and command separation.
- [ ] 4.10 Implement and validate draft-release ordering: image manifests, rendered release version, shell validation, checksum generation, asset attachment verification, then release publication.
- [ ] 4.11 Document that the published checksum verifies artifact integrity within the GitHub trust boundary and is not an independent signature.

## 5. Verification

- [ ] 5.1 Add `tests/esdiag-local.sh` covering shellcheck, help/version, repository-independent execution, generated configuration, secure/insecure startup, setup, repeated `up`, status/auth, raw secret output, clipboard selection and opt-out, down, reset, update checks, and safe self-replacement.
- [ ] 5.2 Update `tests/bin/esdiag-control.sh` to prove delegated startup uses `ESDIAG_IMAGE_TAG=esdiag:${version}` for both setup and service containers with image pulling disabled.
- [ ] 5.3 Validate generated Compose configurations with Docker Compose v2 and Podman Compose and run available provider integration suites with isolated state directories.
- [ ] 5.4 Run `shellcheck bin/esdiag-local bin/esdiag-control tests/esdiag-local.sh tests/bin/esdiag-control.sh` and resolve all findings.
- [ ] 5.5 Run `cargo clippy` and resolve any warnings.
- [ ] 5.6 Run `cargo test` and confirm no Rust regressions.
- [ ] 5.7 Run `OPENSPEC_TELEMETRY=0 openspec validate add-standalone-esdiag-local --strict`.
- [ ] 5.8 Test self-update from a temporary copied script with fixture-backed fake network/checksum tools, including PATH invocation, spaces, symlink refusal, verification failure, and proof that the repository script is unchanged.
- [ ] 5.9 Test Linux and macOS adapter branches, secure `.env` parsing, configurable port validation, state separation, credential/volume mismatch failures, and script-versus-stack upgrade behavior.
