## 1. Timing baseline

- [x] 1.1 Add `bin/build-timings.sh` with the `server`, `core`, `cli`, `server-tests`, and `clean` scenarios, a warm-up run, `--runs N`, file-path arguments, and median output; verify `shellcheck bin/build-timings.sh` passes and a run prints one median per scenario and leaves `git status` clean
- [x] 1.2 On the pre-split commit, run `bin/build-timings.sh` with the current-layout file paths and save the table for the PR description; verify all five scenarios report a median
- [x] 1.3 Capture baseline behavior artifacts: help output of every `esdiag` command and subcommand, and directory output for each archive in `tests/archives/`; store them outside the repo and verify they regenerate identically on a second run apart from run IDs and timestamps

## 2. Workspace scaffolding

- [x] 2.1 Add `[workspace.package]` (version, edition, rust-version, license, repository, homepage) and `[workspace.dependencies]` to the root `Cargo.toml`; switch the root and `repo-tools` manifests to workspace entries, leaving `elasticrc`'s version and license independent; verify `cargo tree -d` shows no new duplicate versions and `cargo build --workspace` succeeds

## 3. esdiag-core

- [x] 3.1 Move `src/` except `main.rs`, `local.rs`, `local/`, and `server/` into `crates/esdiag-core/src/` with `git mv`; add `elasticrc`, `keystore`, `server`, `setup`, `agent`, and `test-support` features; move `TestEnv` behind `test-support` and the Elasticsearch mock into `esdiag-core`'s test-only code, included by path from `tests/common/`; verify `cargo test -p esdiag-core --features test-support` passes
- [x] 3.2 Add `bin/sync-crate-assets.sh` with `--check` and the `esdiag-core` build script that selects the asset root, builds the Kibana assets zip under `setup`, and exports the root for `include_str!` and `rust-embed`; add `crates/*/bundled/` to `.gitignore`; verify `--check` passes after a sync and fails after editing a canonical asset, and `cargo package -p esdiag-core --list` includes `bundled/assets/` and `bundled/skill/`
- [x] 3.3 Turn the root `lib.rs` into the facade over `esdiag-core`; verify `cargo build` and `cargo test` pass with default features
- [x] 3.4 Point `crates/repo-tools` at `esdiag-core`; verify `cargo build -p repo-tools -v` does not compile `esdiag-gui` and `esdiag-lite-generate` output is unchanged

## 4. esdiag-gui

- [x] 4.1 Move `src/server/` to `crates/esdiag-gui/src/server/` and `templates/` to `crates/esdiag-gui/templates/` with `git mv`; move `DocsAssets` and `ServerAssets`; add the asset-root build script for `docs/`; add `setup`, `keystore`, `elasticrc`, and `desktop` features; make the `pub(crate)` library items the server uses `pub`; verify `cargo test -p esdiag-gui --all-features` passes
- [ ] 4.2 Move the Tauri wrapper from `main.rs` into `esdiag_gui::desktop::run(tauri::Context)` and `desktop_startup_tests` into `esdiag-gui`; keep `tauri::generate_context!()`, `tauri-build`, `tauri.conf.json`, and `desktop/` in the root package; verify `cargo test -p esdiag-gui --features desktop` passes and the desktop app launched with `cargo run --features desktop` opens the Web UI in its window and stops the server when the window closes
- [x] 4.3 Wire the root features as in design.md and re-export `esdiag_gui::server` from the facade; verify `cargo build` succeeds with default features, with `--no-default-features`, and with `--no-default-features` plus each of `agent`, `server`, `setup`, `keystore`, and `elasticrc`
- [x] 4.4 Verify `cargo build --no-default-features -v` compiles neither `esdiag-gui` nor Tauri, `cargo build --no-default-features --features server -v` compiles `esdiag-gui` but not Tauri, and `cargo tauri build --features desktop` produces the host bundle
- [x] 4.5 Verify rebuild scope: after a full build, touch a file in `esdiag-gui`, run `cargo build -v`, and confirm `esdiag-core` does not show `Compiling`
- [x] 4.6 Verify `esdiag serve` loads the Web UI, renders docs, and serves static assets

## 5. Timing gate

- [x] 5.1 Run `bin/build-timings.sh` with the new-layout file paths on the same host as 1.2; verify the server `cargo build` median is at least 30% below baseline, the core and cli medians and clean build are no more than 10% above baseline, and the server-tests median is not above baseline. If the server gate fails, stop and revisit the change before continuing

## 6. Publication

- [x] 6.1 Declare `esdiag-core` and `esdiag-gui` in `[workspace.dependencies]` with exact `=` version requirements; add implementation-crate descriptions and READMEs; add the root `package.include`; verify all three manifests share one version and license
- [x] 6.2 Run `bin/sync-crate-assets.sh`, then, on a clean worktree, `cargo publish --workspace --exclude elasticrc --exclude repo-tools --allow-dirty --dry-run`; verify all three packages package and build from their tarballs and neither `elasticrc` nor `repo-tools` is included
- [ ] 6.3 Add a CI job that checks the layering rules with `cargo metadata`, runs `bin/sync-crate-assets.sh --check` after a sync, and runs the publish dry run on stable; verify it passes on the PR

## 7. Verification

- [x] 7.1 Compare help output and archive directory output against the 1.3 baseline; verify they are identical apart from run IDs and timestamps
- [x] 7.2 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features`, and `cargo test --workspace`; verify all pass with no new warnings
- [x] 7.3 Record the 1.2 and 5.1 timing tables in the PR description

## 8. Documentation and release process

- [x] 8.1 Update `.agents/skills/esdiag-release/` (`SKILL.md`, `references/crates-io.md`, `references/branching.md`) for the three-package publish, the asset sync step, the exact `=` requirements as a version surface, and partial-publish recovery; verify each documented command runs in dry-run form
- [x] 8.2 Update `docs/repository/organization.md`, `docs/build/desktop-packaging.md`, and `AGENTS.md` key paths for `crates/esdiag-core/`, `crates/esdiag-gui/`, and the moved `templates/`; run `okf validate docs/`; verify every path named in those files exists
