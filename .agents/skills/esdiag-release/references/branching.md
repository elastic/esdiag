# Release Branching

Use this reference to split the next development cycle from a release candidate.
This prepares two PRs and draft notes; it does not publish artifacts or create a
release tag.

## Establish The Cut

Record the exact upstream main commit and these values before editing:

```text
PREVIOUS=0.16.5
SERIES=0.17
RELEASE_VERSION=0.17.0-rc1
NEXT_VERSION=0.18.0-SNAPSHOT
```

Fetch upstream and origin with pruning. Preserve local changes; use clean task
worktrees when the current checkout is dirty. Create upstream/SERIES from the
recorded upstream/main commit **before** the next-development version bump.
If the series branch already exists, inspect it instead of resetting it.

## Development PR

1. Create a topic branch from the recorded cut for upstream/main.
2. Set the application version to NEXT_VERSION in Cargo.toml (the
   `[workspace.package]` version and the exact `=` requirements on
   `esdiag-core` and `esdiag-gui` in `[workspace.dependencies]`), Cargo.lock,
   bin/esdiag-local, and current version-sensitive examples and tests. Preserve
   historical fixtures, archived specs, and independently versioned packages.
3. Run cargo upgrade with recursive lockfile updates across workspace manifests.
   Apply compatible updates by default; report incompatible upgrades separately
   unless the requested scope includes their migrations. Respect the declared
   minimum Rust version.
4. Use public registry dependencies, including Elasticsearch, instead of Git
   forks. Reuse the repository's compatibility adapters where needed.
5. Regenerate third-party notices from the resulting lockfile with
   `cargo about generate --locked about.hbs -o NOTICES.md`. Keep NOTICES.md
   as the only application notice artifact. Builds use committed notices by
   default; optional source-build generation uses ESDIAG_GENERATE_NOTICE=1.
   Never bypass package verification with --no-verify when a build script
   rewrites packaged sources.
6. Validate, commit, push to the fork, and open a PR targeting upstream/main.

## Release Candidate PR

1. Create a separate topic branch from upstream/SERIES, not from the
   next-development PR.
2. Set RELEASE_VERSION in the same application version surfaces.
3. Keep the branch-cut dependency set unless release fixes or registry
   compatibility require updates. Audit all workspace manifests, Cargo.lock,
   and resolved Cargo metadata for Git dependencies and private registries.
   Local workspace packages are valid; production path dependencies need
   published version requirements and a successful registry package build.
4. Resolve registry compatibility, regenerate notices, run
   `bin/sync-crate-assets.sh`, and on a clean worktree verify the actual
   packages with
   `cargo publish --dry-run --locked --allow-dirty --workspace --exclude elasticrc --exclude repo-tools`
   (see the crates.io reference for why `--allow-dirty` is needed).
5. Run formatting, workspace compilation and tests, feature-combination checks,
   and relevant launcher tests. Report any omitted platform or live tests.
6. Commit, push, and open a PR targeting upstream/SERIES. Do not directly push
   release-candidate changes over branch protections.

## Draft Notes And Handoff

- Use CHANGELOG.md entries after PREVIOUS as the curated baseline. Verify
  important references against merged PRs; include breaking CLI output changes,
  saved-data migrations, credential changes, and deployment compatibility.
- End the notes with a link to the series branch CHANGELOG.md.
- Check whether PREVIOUS is an ancestor of the cut. Set the release branch's
  RELEASE_NOTES_START_TAG explicitly when it is not; do not trust GitHub's
  automatically inferred range.
- Store notes in `docs/release_notes/SERIES.md` with the repository's OKF YAML
  frontmatter: `type: Reference`, title, description, tags, `status: draft`,
  and `version: RELEASE_VERSION`. Keep the series filename stable as candidates
  change; update the version, title, and heading together.
- Link the notes from `docs/documentation.md`. Keep status in frontmatter
  rather than duplicating it in prose. At official publication, update the
  version to the published version and set `status: published`; a published RC
  is still a prerelease and must not be presented as a stable release.
- Commit draft notes with the release PR. A GitHub draft can reserve the future
  release tag without pushing that tag; mark RC drafts as prereleases and record
  that their final target must be verified after the PR merges.
- Preserve curated draft notes when the eventual tag workflow runs.
- Report both PR URLs, branch-cut SHA, dependency policy, validation results,
  and draft URL. The series branch remains at the cut until its PR merges.
- After merge, refresh both branch tips and verify versions before tagging or
  building artifacts. Publishing remains a separate release-target workflow.
- RC images must not replace stable latest/series aliases or the stable
  Homebrew formula. Build images before triggering a workflow that verifies
  them, and publish only with the applicable human authorization.
