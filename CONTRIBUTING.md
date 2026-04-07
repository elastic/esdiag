Contributing
============

All contributions are welcome. Diagnosing and troubleshooting issues is a challenging task, and improvements come in many forms.

Not all contributions need to be code. Design feedback, source coverage ideas, API research, documentation updates, and dashboard ideas are all valuable. Figuring out which Elastic Stack APIs provide the most useful signal, and how to represent that data as JSON documents for Kibana visualizations, is a meaningful contribution.

Before You Start
----------------

- Check for an existing issue before opening a new one.
- Use the available issue templates for bug reports, feature requests, and dashboard requests when they fit your change.
- If you plan to work on a larger change, open an issue or draft PR early so the approach can be discussed before too much code is written.

Local Setup
-----------

This repository is a Rust project and currently requires Rust `1.89` or newer.

1. Install the Rust toolchain with `rustup`.
2. Clone the repository.
3. Build the project from the repository root:

   ```sh
   cargo build
   ```

If you want a fully local stack for manual testing, use:

```sh
./bin/esdiag-control up
```

That command starts the local services needed to exercise the web UI, Elasticsearch, and Kibana workflow described in `readme.md`.

Before Opening a PR
-------------------

Run the checks that cover your change from the repository root:

```sh
cargo fmt
cargo clippy
cargo test
```

Also:

- Add or update documentation when behavior, commands, or workflows change.
- Add or update tests when they materially reduce regression risk.
- Include screenshots or UI notes when changing the browser interface or dashboards.
- Keep PRs focused on one logical change when possible.

Pull Request Workflow
---------------------

- Create your working branch from `preview`.
- Open pull requests against `preview` unless a maintainer asks you to target a different branch.
- Rebase your branch as needed to keep it current.
- Maintainers handle the final merge strategy and release flow.

If you need the full maintainer branch and release policy, see `docs/repository/branch-management.md`.

Sign the Contributor License Agreement
--------------------------------------

Be sure to sign the [Contributor License Agreement](https://www.elastic.co/contributor-agreement) before you submit your first PR.
