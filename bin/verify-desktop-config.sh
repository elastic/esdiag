#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_CONF="${ROOT_DIR}/tauri.conf.json"
TARGETS_CONF="${ROOT_DIR}/packaging/desktop-targets.json"
FLATPAK_MANIFEST="${ROOT_DIR}/packaging/flatpak/com.elastic.esdiag.json"

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require jq

if [[ ! -f "${TAURI_CONF}" ]]; then
  echo "Missing tauri config: ${TAURI_CONF}" >&2
  exit 1
fi

if [[ ! -f "${TARGETS_CONF}" ]]; then
  echo "Missing desktop targets config: ${TARGETS_CONF}" >&2
  exit 1
fi

if [[ ! -f "${FLATPAK_MANIFEST}" ]]; then
  echo "Missing flatpak manifest: ${FLATPAK_MANIFEST}" >&2
  exit 1
fi

WINDOWS_BUNDLE_FORMAT="$(jq -r '.windows.bundleFormat' "${TARGETS_CONF}")"
WINDOWS_MIN_VERSION="$(jq -r '.windows.minimumVersion' "${TARGETS_CONF}")"
FLATPAK_BASE_VERSION="$(jq -r '.flatpak.baseAppVersion' "${TARGETS_CONF}")"
FLATPAK_LOCAL_ONLY="$(jq -r '.flatpak.localOnly' "${TARGETS_CONF}")"

if [[ "${WINDOWS_BUNDLE_FORMAT}" != "msi" ]]; then
  echo "desktop-targets.json must set windows.bundleFormat to msi" >&2
  exit 1
fi

if [[ "${WINDOWS_MIN_VERSION}" != "10" ]]; then
  echo "desktop-targets.json must set windows.minimumVersion to 10" >&2
  exit 1
fi

if [[ "${FLATPAK_BASE_VERSION}" != "0.15.0" ]]; then
  echo "desktop-targets.json must set flatpak.baseAppVersion to 0.15.0" >&2
  exit 1
fi

if [[ "${FLATPAK_LOCAL_ONLY}" != "true" ]]; then
  echo "desktop-targets.json must keep flatpak.localOnly=true for this scope" >&2
  exit 1
fi

jq -e '.bundle.targets | type == "array"' "${TAURI_CONF}" >/dev/null
jq -e '.bundle.targets | index("msi") != null' "${TAURI_CONF}" >/dev/null
jq -e '.bundle.targets | index("nsis") == null' "${TAURI_CONF}" >/dev/null

jq -e --arg v "${FLATPAK_BASE_VERSION}" '."x-esdiag-base-app-version" == $v' "${FLATPAK_MANIFEST}" >/dev/null

echo "Desktop packaging config validated:"
echo "- Windows minimum version: ${WINDOWS_MIN_VERSION}"
echo "- Windows bundle format: ${WINDOWS_BUNDLE_FORMAT}"
echo "- Flatpak base app version: ${FLATPAK_BASE_VERSION}"
echo "- Flatpak local-only scope: ${FLATPAK_LOCAL_ONLY}"
