# Issue tracker: GitHub

Issues and PRDs for this repo live as GitHub issues on the **`upstream` remote, `elastic/esdiag`** — not the `origin` fork (`VimCommando/esdiag`). Use the `gh` CLI for all operations.

> **Always pass `--repo elastic/esdiag`.** `gh` infers the repo from `origin` when run inside a clone, which here is the fork. Every command below targets upstream explicitly so work lands on `elastic/esdiag`.

## Conventions

- **Create an issue**: `gh issue create --repo elastic/esdiag --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --repo elastic/esdiag --comments`, filtering comments by `jq` and also fetching labels.
- **List issues**: `gh issue list --repo elastic/esdiag --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Comment on an issue**: `gh issue comment <number> --repo elastic/esdiag --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --repo elastic/esdiag --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --repo elastic/esdiag --comment "..."`

## Pull requests as a triage surface

**PRs as a request surface: yes.** External PRs run through the same labels and states as issues, using the `gh pr` equivalents (all with `--repo elastic/esdiag`):

- **Read a PR**: `gh pr view <number> --repo elastic/esdiag --comments` and `gh pr diff <number> --repo elastic/esdiag` for the diff.
- **List external PRs for triage**: `gh pr list --repo elastic/esdiag --state open --json number,title,body,labels,author,authorAssociation,comments` then keep only `authorAssociation` of `CONTRIBUTOR`, `FIRST_TIME_CONTRIBUTOR`, or `NONE` (drop `OWNER`/`MEMBER`/`COLLABORATOR` — collaborators' in-flight PRs are left alone).
- **Comment / label / close**: `gh pr comment --repo elastic/esdiag`, `gh pr edit --repo elastic/esdiag --add-label`/`--remove-label`, `gh pr close --repo elastic/esdiag`.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either — resolve with `gh pr view 42 --repo elastic/esdiag` and fall back to `gh issue view 42 --repo elastic/esdiag`.

## When a skill says "publish to the issue tracker"

Create a GitHub issue on `elastic/esdiag`.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --repo elastic/esdiag --comments`.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **child** issues as tickets. All `gh` commands take `--repo elastic/esdiag`.

- **Map**: a single issue labelled `wayfinder:map`, holding the Notes / Decisions-so-far / Fog body. `gh issue create --repo elastic/esdiag --label wayfinder:map`.
- **Child ticket**: an issue linked to the map as a GitHub sub-issue (`gh api` on the sub-issues endpoint). Where sub-issues aren't enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Labels: `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`), plus `wayfinder:claimed` once claimed.
- **Blocking**: native issue relationships where available; otherwise a `Blocked by: #<n>, #<n>` line at the top of the child body. A ticket is unblocked when every issue it lists is closed.
- **Frontier query**: list the map's open children (`gh issue list --repo elastic/esdiag --state open`, scoped to the map's sub-issues / task list), drop any with an open `Blocked by` issue or the `wayfinder:claimed` label; first in map order wins.
- **Claim**: `gh issue edit <n> --repo elastic/esdiag --add-label wayfinder:claimed` — the session's first write.
- **Resolve**: `gh issue comment <n> --repo elastic/esdiag --body "<answer>"`, then `gh issue close <n> --repo elastic/esdiag`, then append a context pointer (gist + link) to the map's Decisions-so-far.
