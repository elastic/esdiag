# AGENTS.md

## Key Paths

- `.agents/skills/esdiag/`: skill for using this project's main binary
- `assets/`: Elastic cluster setup assets, used by `src/setup.rs`
- `bin/`: user-facing helper executables
- `bin/esdiag-control`: user quick-start container deployment script
- `desktop/scripts/`: desktop build and packaging helper scripts
- `docs/`: user and maintainer docs
- `openspec/`: specs and changes
- `src/`: Rust code
- `src/main.rs`: CLI and orchestration
- `src/processor/`: Diagnostic bundle processing and enrichment
- `src/receiver/`: Inputs and collection
- `templates/`: Web UI templates used by `src/server`
- `tests/`: tests

For deeper repository structure see `docs/repository/organization.md`

## Expectations

- Update nearby docs when behavior changes
- Update `CHANGELOG.md` for user-visible changes with `.agents/skills/changelog/SKILL.md`

## Design Patterns

- Prefer Rust typestate for multi-stage workflows.
- Prefer Datastar server PatchElements instead of JavaScript DOM manipulation.
- Prefer Datastar signals over Askama conditionals for HTML element state.
- Prefer semantic HTML and CSS in `templates/`.
