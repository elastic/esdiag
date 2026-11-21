# crates.io Release Target

Use this reference when validating or publishing ESDiag's crates: the
`esdiag` application set, or the standalone `elasticrc` library.

## Package Selection

Set `CRATE` to the release being published.

```bash
CRATE=esdiag
VERSION=0.18.0
```

`CRATE=esdiag` publishes three packages in lockstep: `esdiag-core`,
`esdiag-gui`, and `esdiag`. The first two are implementation crates without a
supported public API; `cargo install esdiag` needs them on crates.io at the
same version. Their version surfaces are the `[workspace.package]` version and
the exact `=VERSION` requirements on `esdiag-core` and `esdiag-gui` in
`[workspace.dependencies]` of the root `Cargo.toml`; keep all three equal.

`CRATE=elasticrc` has a version independent from the ESDiag application. A
standalone `elasticrc` release does not publish the `esdiag` set or change its
version-sensitive release files.

Never run a workspace publish without the package selection below.
`repo-tools` is `publish = false`.

## Package Validation

Run these from the clean release branch after setting `VERSION`. Publishing
several packages in one command needs Cargo 1.90 or newer.

For `CRATE=esdiag`, copy the embedded repository files into the crates first,
then verify all three packages build from their tarballs in dependency order.
Cargo counts the gitignored `bundled/` copies as uncommitted, so the package
commands need `--allow-dirty`; the `git status` guard keeps that flag from
admitting any other uncommitted change:

```bash
bin/sync-crate-assets.sh
bin/sync-crate-assets.sh --check
test -z "$(git status --porcelain)"
cargo publish --dry-run --locked --allow-dirty --workspace --exclude elasticrc --exclude repo-tools
```

For `CRATE=elasticrc`:

```bash
cargo package --locked --package elasticrc
cargo publish --dry-run --locked --package elasticrc
```

These commands validate the actual registry packages and metadata. Resolve any
generated-file changes, including `NOTICES.md`, before committing the release.

## Publication

Require explicit human approval before the actual publication. Read the
crate-specific token without printing or committing it:

```bash
# CRATE=esdiag
test -z "$(git status --porcelain)" && bin/sync-crate-assets.sh --check
CARGO_REGISTRY_TOKEN="$(<"$HOME/.config/apikey/esdiag.crates.io")" \
  cargo publish --locked --allow-dirty --workspace --exclude elasticrc --exclude repo-tools
cargo info "esdiag@$VERSION"

# CRATE=elasticrc
CARGO_REGISTRY_TOKEN="$(<"$HOME/.config/apikey/esdiag.crates.io")" \
  cargo publish --locked --package elasticrc
cargo info "elasticrc@$VERSION"
```

Treat an accepted version as immutable. Cargo verifies every package before
uploading any, but an upload can still fail partway. If some of `esdiag-core`,
`esdiag-gui`, and `esdiag` are published at `VERSION`, publish only the
remaining ones at the same `VERSION`, in that order, from the same commit:

```bash
test -z "$(git status --porcelain)" && bin/sync-crate-assets.sh --check
CARGO_REGISTRY_TOKEN="$(<"$HOME/.config/apikey/esdiag.crates.io")" \
  cargo publish --locked --allow-dirty --package esdiag-gui
```

Never re-publish an accepted package or change its contents. If a published
package needs correction, cut a new patch release of the whole set.
