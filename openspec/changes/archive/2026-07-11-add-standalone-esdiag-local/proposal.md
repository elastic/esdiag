## Why

Running the local ESDiag Elastic Stack currently requires a source checkout because `bin/esdiag-control` reads repository metadata, copies repository-owned environment and Compose files, and builds the ESDiag container image before startup. Users should be able to download one release artifact, run `esdiag-local up` from any directory, and receive a fully configured local Elasticsearch, Kibana, and ESDiag deployment based on official images from `docker.elastic.co`.

Repository-dependent container builds must remain supported for development and for restricted environments that require source-controlled, auditable image builds.

## What Changes

- Add `bin/esdiag-local` as a standalone, location-independent shell script and release artifact.
- Embed the environment and Compose templates needed by `esdiag-local`; generated runtime state is written to a managed local directory and does not require repository files.
- Target Bash on Linux and macOS with platform-aware adapters for path, checksum, resource, clipboard, and browser operations.
- Make `esdiag-local up` pull version-pinned Elasticsearch, Kibana, and ESDiag images from `docker.elastic.co` by default.
- Start Elasticsearch and Kibana first, create ESDiag credentials, run `esdiag setup`, and start the ESDiag web service only after asset setup succeeds.
- Give `esdiag-local` distribution-oriented lifecycle commands for bringing the stack up and down, inspecting it, retrying setup, and removing local state.
- Add script-friendly `secrets password` and `secrets apikey` commands, plus platform-aware clipboard guidance and best-effort password copying before automatic browser launch.
- Keep `bin/esdiag-control` repository-dependent and retain its custom-image `build` and `buildx` workflows.
- Have repository-oriented lifecycle commands delegate to `esdiag-local` with isolated repository state, `ESDIAG_IMAGE_TAG` set to the locally built ESDiag image, and image pulling disabled.
- Treat generated `.env` content as allowlisted data rather than sourced shell code, and make all three host ports configurable there.
- Add an explicit self-update workflow that checks official `github.com/elastic/esdiag` releases and uses `curl` to download, verify, and atomically replace the installed script.
- Add standalone and delegated lifecycle integration coverage in `tests/esdiag-local.sh` and the existing `tests/bin/esdiag-control.sh`.
- Publish the version-pinned `esdiag-local` script and checksum as GitHub release assets only after its referenced ESDiag container image is available.
- Document `https://ela.st/esdiag-local` as the stable human-facing URL for discovering the latest official release and its downloadable assets.

## Capabilities

### New Capabilities
- `standalone-local-stack`: Self-contained local Elastic Stack orchestration using official or explicitly overridden ESDiag container images.

### Modified Capabilities
- None.

## Impact

- **Target Elastic products:** Local Elasticsearch and Kibana containers configured for ESDiag, plus the ESDiag service container.
- **Shell CLI:** Adds `esdiag-local` and changes `esdiag-control` lifecycle implementation to delegate while preserving repository-based custom builds.
- **Credential UX:** Adds deliberate raw-secret output commands while keeping ordinary status and logs free of credentials.
- **Release updates:** Adds opt-in GitHub release checks and checksum-verified self-replacement for the standalone script.
- **Release automation:** Adds GitHub release assets with stable filenames for the version-pinned script and checksum.
- **Container distribution:** Requires a versioned multi-platform ESDiag image in `docker.elastic.co/esdiag/esdiag` before publishing the matching script.
- **Generated local state:** Adds a managed environment file, Compose definition, and failure logs outside the repository checkout.
- **Repository test state:** Keeps source-build deployments under `target/esdiag-local`, separate from the standalone user's state and volumes.
- **Tests:** Adds shell and container integration coverage for standalone execution, official-image startup, custom-image delegation, security modes, idempotency, and teardown.
- **Rust CLI, Web UI, and core processing:** No behavioral changes; the existing `esdiag setup` and `esdiag serve` commands are orchestrated by the new script.
