---
name: esdiag-release
description: Prepare, stage, verify, and troubleshoot ESDiag releases. Use when cutting a numbered release branch, drafting release notes, setting stable versions, publishing multi-platform container tags to Elastic registries, creating a numeric release tag, generating the standalone esdiag-local artifact, or validating a draft GitHub release before human publication.
---

# ESDiag Release

Prepare an ESDiag release through a verified GitHub draft. Never publish the GitHub release without explicit human approval.

## Release Variables

Establish these values before changing state:

```text
VERSION=0.16.0
SERIES=0.16
PREVIOUS=0.15.0
BRANCH=0.16
TAG=0.16.0
```

ESDiag uses numeric tags without a leading `v`. Keep `VERSION`, the Cargo package version, `bin/esdiag-local`, the full image tag, and `TAG` aligned.

## 1. Preflight

1. Require a clean worktree. Preserve unrelated user changes.
2. Fetch `upstream` and `origin` with pruning.
3. Fast-forward local `main` to `upstream/main`; push `origin/main` if the fork is behind.
4. Review `CHANGELOG.md`, commits, merged PRs, existing releases, tags, and the previous release branch.
5. Check ancestry explicitly:

```bash
git merge-base --is-ancestor "$PREVIOUS" upstream/main
```

Release commits commonly live only on maintenance branches. A non-ancestor previous tag is valid, but GitHub cannot infer the desired release-note baseline from topology.

## 2. Create The Release Branch

Create `BRANCH` from the exact verified `upstream/main` commit and push it to `upstream`:

```bash
git switch -c "$BRANCH" upstream/main
git push --set-upstream upstream "$BRANCH"
```

If branch rules require a PR, use a PR. Do not bypass protections unless the user has authority and explicitly requests it.

## 3. Set The Stable Version

Replace the active snapshot version with `VERSION` in:

- `Cargo.toml` and the ESDiag package entry in `Cargo.lock`
- `bin/esdiag-local`
- current examples and version-sensitive tests
- generated `NOTICE.txt`

Search the repository for stale current-version `SNAPSHOT` references. Do not replace historical fixtures or independently pinned packaging versions merely because they contain an older version.

Run at least:

```bash
cargo fmt --all -- --check
cargo check
cargo test
shellcheck bin/esdiag-local
bash tests/esdiag-local.sh
bash tests/bin/esdiag-control.sh
git diff --check
```

Fix upstream failures that block release validation before tagging. Commit and push all release-branch fixes.

## 4. Draft Release Notes

Build notes from changes after `PREVIOUS`, using the changelog skill and verified merged PR metadata. Emphasize user-visible features, compatibility changes, operational changes, and important fixes. Keep maintenance details secondary.

End curated notes with a link to the release branch `CHANGELOG.md`. Keep the GitHub release in draft state.

In `.github/workflows/release-esdiag-local.yml`:

- Trigger numeric tags with `"[0-9]*.[0-9]*.[0-9]*"`.
- Create the release with `--notes-start-tag "$PREVIOUS"` for the release branch.
- Leave the release as a draft after verification.
- Never run `gh release edit ... --draft=false` in automation.

The previous tag must be explicit when it is not an ancestor of the new tag; otherwise GitHub may select an older reachable release and include unrelated changes.

## 5. Publish Container Manifests

Build once for `linux/amd64,linux/arm64` and push identical tags to both registries:

```text
docker.elastic.co/esdiag/esdiag:SERIES
docker.elastic.co/esdiag/esdiag:VERSION
docker.elastic.co/esdiag/esdiag:latest
us-west1-docker.pkg.dev/elastic-ce-tools/esdiag/esdiag:SERIES
us-west1-docker.pkg.dev/elastic-ce-tools/esdiag/esdiag:VERSION
us-west1-docker.pkg.dev/elastic-ce-tools/esdiag/esdiag:latest
```

Prefer one `docker buildx build --push` with all six tags. If `SERIES` and `latest` already exist, create `VERSION` aliases with `docker buildx imagetools create`; do not rebuild.

Inspect every remote tag. Require:

- the same OCI index digest across registries and aliases
- `linux/amd64` and `linux/arm64` manifests
- a runtime check that the image reports `esdiag VERSION`

The release workflow verifies `docker.elastic.co/esdiag/esdiag:VERSION`, so that full tag must exist before creating `TAG`.

## 6. Create The Numeric Tag

Tag the final, pushed release-branch commit, not an earlier preparation commit:

```bash
git tag -a "$TAG" -m "Release $VERSION"
git push upstream "$TAG"
```

Verify the remote annotated tag dereferences to the release branch tip.

## 7. Monitor The Draft Workflow

Watch the `Release esdiag-local` workflow through completion. It must:

1. Create or reuse a draft GitHub release.
2. Verify the full-version container manifests.
3. Render `esdiag-local` with `VERSION`.
4. Run syntax and ShellCheck validation.
5. Generate `esdiag-local.sha256`.
6. Upload and download both assets.
7. Verify the checksum.
8. Confirm the release remains a draft pending human approval.

After success, verify remotely:

- release tag is `TAG`
- `isDraft` is true
- curated notes cover only `PREVIOUS...TAG`
- assets contain `esdiag-local` and `esdiag-local.sha256`
- both assets are in the uploaded state

## 8. Human Publication Gate

Stop after draft verification. Report the draft URL, tag commit, image digest, platforms, tests, and assets. A human reviews and manually publishes the release.

Do not publish, mark non-draft, or silently approve the release.

## Recovery Rules

- **Wrong generated range:** Replace the draft body with curated notes, set `--notes-start-tag PREVIOUS`, and push the workflow fix.
- **Workflow fails after creating the draft:** Inspect failed logs, fix the release branch, and rerun validation locally.
- **Unpublished tag points at an obsolete commit:** Move it only with explicit user approval, force-push only that tag, then monitor the new workflow run.
- **Published tag or release is wrong:** Do not move or delete it. Stop and request a release-management decision.
- **Workflow change rejected:** Confirm GitHub CLI auth includes the `workflow` scope; do not expose tokens.
- **Assets missing:** Treat the release as incomplete even if the draft exists. Inspect the workflow before publication.

## Command Conventions

Follow repository `AGENTS.md`: use `rtk` for supported commands and pipe GitHub JSON/API output through `toon -s`. Use authenticated HTTPS if SSH is unavailable. Keep the worktree clean and ensure local `BRANCH`, `upstream/BRANCH`, and the dereferenced `TAG` commit match before handoff.
