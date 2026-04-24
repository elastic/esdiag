---
name: changelog
description: Maintain `CHANGELOG.md` entries using Keep a Changelog 1.1.0 conventions. Use when adding or reviewing changelog entries, preparing unreleased notes for a PR, or checking whether changelog bullets are correctly scoped and referenced during PR review workflows.
---

# Changelog Management

Use this skill when working on `CHANGELOG.md`, release notes, or PRs that should
update changelog content.

## Standard

- Follow [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).
- Prefer `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, and `Security`.
- Keep entries user-facing. Describe behavior and outcomes, not internal refactors
  unless users/operators would care.
- Write concise bullets with one feature, fix, or change per bullet.

## Project Rules

- Prefer issue numbers over PR numbers on bullets.
- Use a PR number only when there is no explicit issue reference to cite.
- Add `#123` references inline at the end of the bullet whenever they can be
  verified from GitHub history or release notes.
- Only include a `Fixed` bullet if the change clearly closed or resolved a GitHub
  issue, or if release notes explicitly frame it as a bug fix.

## Entry Writing Rules

- `Added`: one bullet per feature, not one bullet per PR.
- `Changed`: use for behavior changes, UX changes, packaging/runtime changes, and
  operator-facing refactors.
- `Fixed`: reserve for bug fixes with verified issue/release-note support.
- `Removed`: use for total feature removal, replaced features go into `changed`.
- `Security`: use for security-focused updates and CVE patches.
- Split combined release-note prose into separate bullets when it actually covers
  multiple distinct features.
- Avoid vague bullets like "misc fixes", "multiple features", or "cleanup".
- Keep tense consistent inside a section.

## Reference Workflow

When updating changelog content:

1. Read the current `CHANGELOG.md`.
2. Identify the target section:
   - `Unreleased` for upcoming work on the active branch.
   - the in-scope release section when preparing a release.
3. Gather references in this order:
   - GitHub release notes
   - linked issues
   - linked PRs
   - branch/tag commit history
4. For each candidate bullet:
   - choose the correct section (`Added`, `Changed`, `Fixed`, etc.)
   - reduce it to a single user-facing item
   - attach an issue number if verified
   - otherwise attach a PR number if verified
5. Remove or rewrite bullets that cannot be supported by the sources.

## GitHub Verification Guidance

- Use release notes to recover issue and PR references that are not obvious from
  commit messages.
- If a commit maps to a PR and that PR says `Resolves #123`, cite `#123` instead
  of the PR number.
- If a PR is the only clean reference, cite the PR number.
- If neither issue nor PR can be verified confidently, omit the reference instead
  of guessing.

## PR Review Use

During PR review workflows, check changelog updates for:

- correct Keep a Changelog structure
- one feature per `Added` bullet
- issue-first reference policy
- `Fixed` bullets limited to verified fixes
- no invented versions or unsupported claims
- accurate `Unreleased` scope for the branch being reviewed

If the PR is missing a changelog update for user-visible behavior, suggest the
smallest accurate entry rather than a long release-summary paragraph.

## Output Pattern

Use this shape when drafting or revising entries:

```markdown
## [Unreleased]

### Added

- Added feature name or user-visible capability (#123).

### Changed

- Changed operator-visible or runtime behavior (#124).

### Fixed

- Fixed the user-visible bug outcome (#125).
```
