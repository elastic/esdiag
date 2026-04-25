#!/usr/bin/env bash
set -euo pipefail

CARGO_TOML="${1:-Cargo.toml}"

if [[ ! -f "${CARGO_TOML}" ]]; then
  echo "Cargo.toml not found: ${CARGO_TOML}" >&2
  exit 1
fi

extract_package_version() {
  local cargo_toml="$1"
  awk '
    $0 == "[package]" { in_package = 1; next }
    /^\[/ {
      if (in_package) {
        exit
      }
    }
    in_package && /^version = "/ {
      line = $0
      sub(/^version = "/, "", line)
      sub(/".*$/, "", line)
      print line
      exit
    }
  ' "${cargo_toml}"
}

package_version="$(extract_package_version "${CARGO_TOML}")"
if [[ -z "${package_version}" ]]; then
  echo "Could not find package version in ${CARGO_TOML}" >&2
  exit 1
fi

if [[ "${package_version}" != *-* ]]; then
  echo "Cargo version ${package_version} is already MSI-safe."
  exit 0
fi

normalized_version="${package_version%%-*}-0"
tmp_file="$(mktemp "${TMPDIR:-/tmp}/cargo-msi-version.XXXXXX")"
trap 'rm -f "${tmp_file}"' EXIT

awk -v normalized="${normalized_version}" '
  BEGIN {
    in_package = 0
    updated = 0
  }
  $0 == "[package]" {
    in_package = 1
    print
    next
  }
  /^\[/ {
    in_package = 0
  }
  in_package && !updated && /^version = "/ {
    print "version = \"" normalized "\""
    updated = 1
    next
  }
  {
    print
  }
  END {
    if (!updated) {
      exit 2
    }
  }
' "${CARGO_TOML}" > "${tmp_file}"

mv "${tmp_file}" "${CARGO_TOML}"
trap - EXIT

echo "Normalized Cargo version from ${package_version} to ${normalized_version} for MSI packaging."
