#!/usr/bin/env bash

# Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
# or more contributor license agreements. Licensed under the Elastic License 2.0.

# Copy the repository files that esdiag-core and esdiag-gui embed into each
# crate's gitignored bundled/ directory, so their published packages are
# self-contained. Canonical files stay where maintainers edit them; build
# scripts read them directly in a checkout and from bundled/ when packaged.

set -euo pipefail

command_name="${0##*/}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
check_only=false

# target directory (relative to the repository) | source directory | entries
bundles=(
    "crates/esdiag-core/bundled/assets|assets|."
    "crates/esdiag-core/bundled/skill|.agents/skills/esdiag|SKILL.md references agents"
    "crates/esdiag-core/bundled/license|.|LICENSE.txt"
    "crates/esdiag-gui/bundled/docs|docs|."
    "crates/esdiag-gui/bundled/license|.|LICENSE.txt"
)

usage() {
    cat <<EOF
Copy embedded repository files into the esdiag-core and esdiag-gui bundled/ directories.

Usage: ${command_name} [--check]

  --check   Fail and name each bundled file that differs from its source
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --check) check_only=true; shift ;;
        --help|-h) usage; exit 0 ;;
        *) printf '%s: unknown option: %s\n' "$command_name" "$1" >&2; exit 1 ;;
    esac
done

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT

status=0
for bundle in "${bundles[@]}"; do
    IFS='|' read -r target source entries <<<"$bundle"
    stage="${staging}/${target}"
    mkdir -p "$stage"
    for entry in $entries; do
        [ -e "${repo_root}/${source}/${entry}" ] || {
            printf '%s: source entry missing: %s/%s\n' "$command_name" "$source" "$entry" >&2
            exit 1
        }
        if [ "$entry" = . ]; then
            cp -R "${repo_root}/${source}/." "$stage/"
        else
            cp -R "${repo_root}/${source}/${entry}" "${stage}/${entry}"
        fi
    done

    if [ "$check_only" = true ]; then
        if [ ! -d "${repo_root}/${target}" ]; then
            printf '%s: %s is missing; run %s\n' "$command_name" "$target" "$command_name" >&2
            status=1
        elif ! differences="$(cd "$repo_root" && diff -rq "$stage" "$target")"; then
            printf '%s: %s is out of date with %s:\n%s\n' "$command_name" "$target" "$source" \
                "${differences//${stage}/${source}}" >&2
            status=1
        fi
        continue
    fi

    rm -rf "${repo_root:?}/${target}"
    mkdir -p "$(dirname "${repo_root}/${target}")"
    cp -R "$stage" "${repo_root}/${target}"
done

if [ "$check_only" = true ]; then
    [ "$status" -eq 0 ] && printf 'Bundled crate assets match their repository sources.\n'
    exit "$status"
fi
printf 'Synchronized bundled crate assets.\n'
