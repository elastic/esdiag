#!/usr/bin/env bash

# Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
# or more contributor license agreements. Licensed under the Elastic License 2.0.

# Measure incremental debug builds for named edit scenarios on this host.
# Each scenario warms the build once, then times --runs builds and prints the
# median. Edits only update modification times; file contents never change.

set -euo pipefail

command_name="${0##*/}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runs=3
all_scenarios=(server core cli server-tests clean)
scenarios=()

first_existing() {
    local candidate
    for candidate in "$@"; do
        if [ -f "${repo_root}/${candidate}" ]; then
            printf '%s\n' "$candidate"
            return
        fi
    done
    printf '%s\n' "$1"
}

server_file="$(first_existing crates/esdiag-gui/src/server/index.rs src/server/index.rs)"
core_file="$(first_existing crates/esdiag-core/src/processor/kibana/mod.rs src/processor/kibana/mod.rs)"
cli_file="src/main.rs"

usage() {
    cat <<EOF
Measure incremental debug build times for edit scenarios on this host.

Usage: ${command_name} [options] [scenario...]

Scenarios (default: all): ${all_scenarios[*]}
  server        touch the server file, then cargo build
  core          touch the core file, then cargo build
  cli           touch the CLI file, then cargo build
  server-tests  touch the server file, then cargo test --no-run
  clean         cargo clean -p for each workspace ESDiag package, then cargo build

Options:
  --runs N            Timed runs per scenario after one warm-up (default: ${runs})
  --server-file PATH  Web server source file (default: ${server_file})
  --core-file PATH    Core processing source file (default: ${core_file})
  --cli-file PATH     CLI binary source file (default: ${cli_file})
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --runs) runs="${2:?--runs needs a value}"; shift 2 ;;
        --server-file) server_file="${2:?--server-file needs a path}"; shift 2 ;;
        --core-file) core_file="${2:?--core-file needs a path}"; shift 2 ;;
        --cli-file) cli_file="${2:?--cli-file needs a path}"; shift 2 ;;
        --help|-h) usage; exit 0 ;;
        -*) printf '%s: unknown option: %s\n' "$command_name" "$1" >&2; exit 1 ;;
        *) scenarios+=("$1"); shift ;;
    esac
done

case "$runs" in
    ''|*[!0-9]*|0) printf '%s: --runs must be a positive integer\n' "$command_name" >&2; exit 1 ;;
esac
[ ${#scenarios[@]} -gt 0 ] || scenarios=("${all_scenarios[@]}")

now() {
    if [ -n "${EPOCHREALTIME:-}" ]; then
        printf '%s\n' "$EPOCHREALTIME"
    else
        perl -MTime::HiRes=time -e 'printf "%.6f\n", time'
    fi
}

median() {
    sort -n | awk '{ v[NR] = $1 } END { if (NR % 2) printf "%.2f\n", v[(NR + 1) / 2]; else printf "%.2f\n", (v[NR / 2] + v[NR / 2 + 1]) / 2 }'
}

prepare() {
    local scenario=$1 file=$2 package
    case "$scenario" in
        clean)
            for package in esdiag esdiag-core esdiag-gui; do
                if cargo pkgid -q -p "$package" >/dev/null 2>&1; then
                    cargo clean -q -p "$package"
                fi
            done
            ;;
        *) touch "${repo_root}/${file}" ;;
    esac
}

build() {
    case "$1" in
        server-tests) cargo test -q --no-run >/dev/null 2>&1 ;;
        *) cargo build -q >/dev/null 2>&1 ;;
    esac
}

cd "$repo_root"
printf '%-14s %-50s %s\n' scenario file "median_s (runs=${runs})"

for scenario in "${scenarios[@]}"; do
    case "$scenario" in
        server|server-tests) file=$server_file ;;
        core) file=$core_file ;;
        cli) file=$cli_file ;;
        clean) file=- ;;
        *) printf '%s: unknown scenario: %s\n' "$command_name" "$scenario" >&2; exit 1 ;;
    esac
    if [ "$file" != - ] && [ ! -f "$file" ]; then
        printf '%s: %s file not found: %s\n' "$command_name" "$scenario" "$file" >&2
        exit 1
    fi

    prepare "$scenario" "$file"
    build "$scenario" || { printf '%s: %s warm-up build failed\n' "$command_name" "$scenario" >&2; exit 1; }

    durations=()
    for _ in $(seq "$runs"); do
        prepare "$scenario" "$file"
        start=$(now)
        build "$scenario" || { printf '%s: %s build failed\n' "$command_name" "$scenario" >&2; exit 1; }
        end=$(now)
        durations+=("$(awk -v s="$start" -v e="$end" 'BEGIN { printf "%.3f\n", e - s }')")
    done

    printf '%-14s %-50s %s\n' "$scenario" "$file" "$(printf '%s\n' "${durations[@]}" | median)"
done
