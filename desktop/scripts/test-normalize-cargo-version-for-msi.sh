#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/desktop/scripts/normalize-cargo-version-for-msi.sh"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/normalize-msi-test.XXXXXX")"
trap 'rm -rf "${TMP_DIR}"' EXIT

assert_contains() {
  local file="$1"
  local pattern="$2"

  if ! grep -Fq -- "${pattern}" "${file}"; then
    echo "Expected ${file} to contain: ${pattern}" >&2
    exit 1
  fi
}

assert_not_contains() {
  local file="$1"
  local pattern="$2"

  if grep -Fq -- "${pattern}" "${file}"; then
    echo "Expected ${file} to not contain: ${pattern}" >&2
    exit 1
  fi
}

cat > "${TMP_DIR}/snapshot.Cargo.toml" <<'EOF'
[package]
name = "esdiag"
version = "0.15.0-SNAPSHOT"
edition = "2024"

[dependencies]
serde = { version = "1.0.228", features = ["derive"] }
EOF

bash "${SCRIPT}" "${TMP_DIR}/snapshot.Cargo.toml" > "${TMP_DIR}/snapshot.log"
assert_contains "${TMP_DIR}/snapshot.Cargo.toml" 'version = "0.15.0-0"'
assert_not_contains "${TMP_DIR}/snapshot.Cargo.toml" 'version = "0.15.0-SNAPSHOT"'
assert_contains "${TMP_DIR}/snapshot.Cargo.toml" 'serde = { version = "1.0.228", features = ["derive"] }'
assert_contains "${TMP_DIR}/snapshot.log" 'Normalized Cargo version from 0.15.0-SNAPSHOT to 0.15.0-0'

cat > "${TMP_DIR}/stable.Cargo.toml" <<'EOF'
[package]
name = "esdiag"
version = "0.15.0"
edition = "2024"

[dependencies]
serde = { version = "1.0.228", features = ["derive"] }
EOF

bash "${SCRIPT}" "${TMP_DIR}/stable.Cargo.toml" > "${TMP_DIR}/stable.log"
assert_contains "${TMP_DIR}/stable.Cargo.toml" 'version = "0.15.0"'
assert_contains "${TMP_DIR}/stable.log" 'Cargo version 0.15.0 is already MSI-safe.'

cat > "${TMP_DIR}/missing-version.Cargo.toml" <<'EOF'
[package]
name = "esdiag"
edition = "2024"
EOF

if bash "${SCRIPT}" "${TMP_DIR}/missing-version.Cargo.toml" > "${TMP_DIR}/missing.log" 2>&1; then
  echo "Expected missing version test to fail" >&2
  exit 1
fi
assert_contains "${TMP_DIR}/missing.log" 'Could not find package version'

echo "normalize-cargo-version-for-msi tests passed"
