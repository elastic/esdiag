# Homebrew Release Target

Use this reference when attaching ESDiag assets for `elastic/homebrew-tools` or
updating its Formula.

## Release Asset Contract

Attach these assets while the GitHub release is still a draft:

```text
esdiag-VERSION-aarch64-apple-darwin.tar.gz
esdiag-VERSION-aarch64-unknown-linux-gnu.tar.gz
esdiag-VERSION-x86_64-unknown-linux-gnu.tar.gz
esdiag-VERSION-checksums.txt
```

Each archive must contain exactly these root entries:

```text
esdiag
LICENSE.txt
NOTICES.md
```

The checksum file contains exactly one lowercase SHA-256 record per archive,
including the [Windows archive](windows.md), which uploads with these assets.
Published release assets are immutable input to the tap. Do not attach or
replace them after the GitHub release becomes public.

## Build And Verify Assets

Build the Apple Silicon macOS binary on native macOS from tag-equivalent source:

```bash
ESDIAG_GENERATE_NOTICE=0 cargo build --release --locked
target/release/esdiag --version
file target/release/esdiag
```

Extract Linux executables from their matching published container manifests:

```bash
cid="$(docker create --platform linux/amd64 docker.elastic.co/esdiag/esdiag:$VERSION)"
docker cp "$cid":/usr/bin/esdiag /path/to/staging/x86_64-unknown-linux-gnu/esdiag
docker rm "$cid"
```

Repeat for `linux/arm64` into `aarch64-unknown-linux-gnu`. Check each binary
with `file` and run `--version` on its declared platform. Create the archives,
verify their root entries, validate the checksum manifest, then upload all four
assets to the draft:

```bash
gh release upload "$TAG" \
  esdiag-"$VERSION"-aarch64-apple-darwin.tar.gz \
  esdiag-"$VERSION"-aarch64-unknown-linux-gnu.tar.gz \
  esdiag-"$VERSION"-x86_64-unknown-linux-gnu.tar.gz \
  esdiag-"$VERSION"-checksums.txt
```

Intel macOS uses the tagged GitHub source archive rather than a native asset.
It must contain `Cargo.toml`, `Cargo.lock`, `LICENSE.txt`, and `NOTICES.md`, and
declare the release `VERSION`.

## Formula Update

Run only after the GitHub release is public, stable, complete, and immutable:

```bash
cd "$HOMEBREW_REPO"
scripts/update-esdiag.sh "$VERSION"
bash -n scripts/update-esdiag.sh tests/update-esdiag.sh
shellcheck scripts/update-esdiag.sh tests/update-esdiag.sh
tests/update-esdiag.sh
```

The updater verifies the three native archives and checksums, validates the
Intel macOS source archive, and renders `Formula/esdiag.rb`. After registering
the local checkout as `elastic/tools`, run:

```bash
brew style --formula elastic/tools/esdiag
brew audit --strict --online elastic/tools/esdiag
brew install --build-from-source elastic/tools/esdiag
brew test elastic/tools/esdiag
```

Create a branch from the current `elastic/homebrew-tools` `main`, commit only
the Formula update, and open the PR using the release operator's GitHub
identity. Wait for the normal pull-request checks and require every check to
pass. Record the PR URL in the release handoff. The Homebrew target is complete
when that PR exists and its checks pass.

The release operator supplies the exact `VERSION`; release discovery does not
run on a schedule. No bottle publication is required: Apple Silicon macOS and
Linux use verified upstream binaries, while Intel macOS builds the locked
tagged source.

If a public release has missing or incorrect assets, cut a new patch release;
do not mutate the published release.
