#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARTIFACT_ROOT="${ROOT_DIR}/target/artifacts"

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

usage() {
  cat <<'EOF'
Usage: bash desktop/scripts/buildx-windows.sh

Builds a local Windows raw desktop app artifact with docker buildx and writes it into:
- target/artifacts/windows/

Status:
- exports a raw desktop app layout only (`esdiag.exe` + `WebView2Loader.dll`)
- MSI is not produced by buildx; use the native Windows GitHub Actions job

Environment:
- WINDOWS_PLATFORM: docker platform for the Windows builder container (default: linux/amd64)
EOF
}

main() {
  require docker

  case "${1:-}" in
    "" )
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac

  local platform="${WINDOWS_PLATFORM:-linux/amd64}"
  local dest="${ARTIFACT_ROOT}/windows"

  mkdir -p "${dest}"
  rm -f "${dest}/esdiag.exe"
  rm -f "${dest}/WebView2Loader.dll"

  docker buildx build \
    --platform "${platform}" \
    --file "${ROOT_DIR}/docker/buildx-windows.Dockerfile" \
    --target artifacts \
    --output "type=local,dest=${dest}" \
    "${ROOT_DIR}"

  echo "Windows raw desktop artifact exported to:"
  echo "- ${dest}/esdiag.exe"
  echo "- ${dest}/WebView2Loader.dll"
  echo "MSI packaging is not produced by buildx; use the Windows GitHub Actions job for the official installer."
}

main "$@"
