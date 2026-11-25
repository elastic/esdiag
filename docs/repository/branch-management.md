---
type: Maintainer Guide
title: Branch Management
description: Maintainer guide to branch strategy and release flow.
tags: [repository, branching, release]
---

Branch Management
=================

This document captures the branch strategy and release flow used by maintainers.

Branch Roles
------------

This repository uses three branch types with clearly defined roles:

- `main`: the primary integration branch for upcoming releases. New features and fixes land here via pull requests.
- Release (`x.y`): long-lived release maintenance branches such as `0.16` and `0.17`, cut from `main` for a release line. These are used for version-specific releases and hotfixes without pulling in newer work from `main`.
- Feature (`feature/*`, `fix/*`, etc.): working branches for day-to-day development and integrating work-in-progress before opening a pull request against `main`.

Normal Development
------------------

- Create a fork of the repository.
- Create feature branches from `upstream/main`.
- Integrate work-in-progress on a feature branch; rebase forked branches freely to keep them current and clean.
- Open pull requests against `upstream/main` and merge with a squash merge.
- Branch protections forbid direct commits to `main` or any `x.y` release branch.

Releases
--------

- Cut a new `x.y` release branch from `main` when preparing a minor release, or target the existing `x.y` branch for a patch release.
- Finalize release version bumps and changelog entries on the release branch via pull request.
- Tag the release from the `x.y` release branch.

Hotfixes
--------

- Land the fix on `main` first (or branch from the affected `x.y` release branch when an urgent production fix cannot wait).
- Keep hotfixes narrowly scoped and production-safe.
- Backport fixes from `main` to affected `x.y` release branches, or immediately forward-port to `main` if the fix landed on `x.y` first.

Summary
-------

In short: work-in-progress integrates on feature branches and merges into `main`, releases ship from `x.y` release branches cut from `main`, and hotfixes are ported between `main` and active `x.y` release branches.
