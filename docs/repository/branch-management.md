Branch Management
=================

This document captures the long-lived branch strategy and release flow used by maintainers.

Branch Roles
------------

This repository uses a small set of long-lived branches with clearly defined roles:

- `main`: the latest stable release line. Every commit here should be releasable.
- `preview`: the integration branch for upcoming work. New features and fixes land here first.
- `x.y`: long-lived release maintenance branches such as `0.13` and `0.14`. These are used for version-specific hotfixes that may ship directly to production without pulling in newer work from `main`.
- `feature/*` and `fix/*`: short-lived working branches for day-to-day development.

Normal Development
------------------

- Create a fork of the repository.
- Create feature branches from `upstream/preview`.
- Rebase forked branches freely to keep them current and clean.
- Merge feature branches into `upstream/preview` with a squash merge.
- Branch protections forbid commits to `main`, `preview`, or any `0.x` branch.

Releases
--------

- Keep `preview` ready for the next release by integrating and validating new work there.
- Before releasing, bring `preview` up to date with `main` so stable fixes already merged to `main` are included in the release candidate.
- Open a PR from `preview` to `main`.
- Merge that PR with a merge commit to preserve a clear release boundary.
- After the `preview` -> `main` PR merges, fast-forward `preview` to the new `main` tip so both long-lived branches point to the same commit before new preview-only work resumes.
- Do not rebase `preview` after a release PR merge, and do not create a follow-up sync merge commit unless branch protections leave no other option.
- Tag the release from `main` and create the new `0.x` maintenance branch from that release point.

Hotfixes
--------

- Branch from the affected stable branch, either `main` or the relevant `0.x` branch.
- Keep hotfixes narrowly scoped and production-safe.
- Merge the hotfix into the affected stable branch and release from there.
- Immediately forward-port the same fix to `main` if the fix landed on a `0.x` branch.
- Ensure `preview` also receives the fix so it is not lost in the next release.

Summary
-------

In short: new work flows into `preview`, releases flow from `preview` to `main`, and hotfixes flow forward from older stable branches into newer ones.
