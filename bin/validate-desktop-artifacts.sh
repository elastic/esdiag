#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_ROOT="${1:-target/artifacts}"

if [[ ! -d "${ARTIFACT_ROOT}" ]]; then
  echo "Artifact root not found: ${ARTIFACT_ROOT}" >&2
  exit 1
fi

require_file() {
  local pattern="$1"
  local label="$2"
  shopt -s nullglob
  local matches=( ${pattern} )
  shopt -u nullglob
  if (( ${#matches[@]} == 0 )); then
    echo "Missing required artifact: ${label} (${pattern})" >&2
    exit 1
  fi
}

require_file "${ARTIFACT_ROOT}/macos"'/ESDiag*.dmg' "macOS dmg"
require_file "${ARTIFACT_ROOT}/windows"'/esdiag.exe' "Windows executable"
require_file "${ARTIFACT_ROOT}/flatpak"'/com.elastic.esdiag-0.15.0.flatpak' "Flatpak bundle"

echo "Desktop artifacts validated:"
echo "- macOS dmg"
echo "- Windows executable"
echo "- Linux flatpak (0.15.0)"
