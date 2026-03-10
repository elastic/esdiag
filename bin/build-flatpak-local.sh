#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGETS_CONF="${ROOT_DIR}/packaging/desktop-targets.json"
MANIFEST="${ROOT_DIR}/packaging/flatpak/com.elastic.esdiag.json"
OUTPUT_DIR="${ROOT_DIR}/target/flatpak"
BUILD_DIR="${OUTPUT_DIR}/build"
REPO_DIR="${OUTPUT_DIR}/repo"

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require jq
require flatpak
require flatpak-builder

if [[ ! -f "${TARGETS_CONF}" || ! -f "${MANIFEST}" ]]; then
  echo "Flatpak packaging config is incomplete." >&2
  exit 1
fi

LOCAL_ONLY="$(jq -r '.flatpak.localOnly' "${TARGETS_CONF}")"
BASE_VERSION="$(jq -r '.flatpak.baseAppVersion' "${TARGETS_CONF}")"
ARTIFACT_NAME="$(jq -r '.flatpak.artifactName' "${TARGETS_CONF}")"

if [[ "${LOCAL_ONLY}" != "true" ]]; then
  echo "This workflow is local-only; refusing to continue with localOnly=${LOCAL_ONLY}." >&2
  exit 1
fi

mkdir -p "${OUTPUT_DIR}"

flatpak-builder \
  --user \
  --force-clean \
  --repo="${REPO_DIR}" \
  "${BUILD_DIR}" \
  "${MANIFEST}"

flatpak build-bundle \
  "${REPO_DIR}" \
  "${OUTPUT_DIR}/${ARTIFACT_NAME}" \
  com.elastic.esdiag \
  stable

echo "Built local Flatpak artifact:"
echo "- ${OUTPUT_DIR}/${ARTIFACT_NAME}"
echo "- base app version ${BASE_VERSION}"
