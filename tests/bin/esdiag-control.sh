#!/usr/bin/env bash

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
assert_contains() { grep -Fq -- "$2" "$1" || fail "$1 does not contain: $2"; }
assert_not_contains() { ! grep -Fq -- "$2" "$1" || fail "$1 contains unexpected text: $2"; }

mkdir -p "$tmp/repo/bin" "$tmp/repo/docker" "$tmp/bin"
cp "$root/bin/esdiag-control" "$tmp/repo/bin/esdiag-control"
chmod 755 "$tmp/repo/bin/esdiag-control"
printf '%s\n' '[package]' 'version = "0.16.0"' >"$tmp/repo/Cargo.toml"
: >"$tmp/repo/docker/Dockerfile"
cat >"$tmp/repo/bin/esdiag-local" <<'EOF'
#!/usr/bin/env bash
printf 'image=%s\nargs=%s\nstate=%s\n' "${ESDIAG_IMAGE_TAG:-}" "$*" "${ESDIAG_LOCAL_DIR:-}" >>"$DELEGATE_LOG"
EOF
cat >"$tmp/bin/podman" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$RUNTIME_LOG"
[[ "$1 $2" == "image inspect" ]] && exit "${IMAGE_EXISTS:-1}"
exit 0
EOF
chmod 755 "$tmp/repo/bin/esdiag-local" "$tmp/bin/podman"
export DELEGATE_LOG="$tmp/delegate.log" RUNTIME_LOG="$tmp/runtime.log"

if command -v shellcheck >/dev/null 2>&1; then shellcheck "$root/bin/esdiag-control" "$root/tests/bin/esdiag-control.sh"; fi
(cd "$tmp/repo" && PATH="$tmp/bin:$PATH" bin/esdiag-control help >"$tmp/help")
assert_contains "$tmp/help" 'Usage:'
assert_not_contains "$tmp/help" '--insecure'
if (cd "$tmp/repo" && PATH="$tmp/bin:$PATH" bin/esdiag-control up --runtime podman --insecure) 2>"$tmp/insecure-error"; then fail '--insecure was accepted'; fi
assert_contains "$tmp/insecure-error" 'Unknown option: --insecure'

(cd "$tmp/repo" && PATH="$tmp/bin:$PATH" bin/esdiag-control up --runtime podman --open-browser=false)
assert_contains "$tmp/runtime.log" 'build --file docker/Dockerfile . --tag esdiag:latest --tag esdiag:0.16.0'
assert_contains "$tmp/delegate.log" 'image=esdiag:0.16.0'
assert_contains "$tmp/delegate.log" 'args=up'
assert_contains "$tmp/delegate.log" '--pull never'
assert_contains "$tmp/delegate.log" 'repo/target/esdiag-local'

: >"$tmp/delegate.log"
(cd "$tmp/repo" && PATH="$tmp/bin:$PATH" ESDIAG_LOCAL_DIR="$tmp/custom state" bin/esdiag-control setup --runtime podman)
assert_contains "$tmp/delegate.log" 'image=esdiag:0.16.0'
assert_contains "$tmp/delegate.log" 'args=setup'
assert_contains "$tmp/delegate.log" '--pull never'
assert_contains "$tmp/delegate.log" "--state-dir $tmp/custom state"

printf 'esdiag-control tests passed\n'
