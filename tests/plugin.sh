#!/usr/bin/env bash

# Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
# or more contributor license agreements. Licensed under the Elastic License 2.0.

# Non-networked packaging checks for the portable ESDiag Agent Skill.

set -uo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
source_skill="${root}/.agents/skills/esdiag"
bundled_skill="${root}/plugin/skills/esdiag"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

only=""
[[ "${1:-}" == "--only" ]] && only="${2:-}"
passed=0
failed=0

fail() { printf '  FAIL: %s\n' "$*" >&2; failed=$((failed + 1)); return 1; }
assert_eq() { [[ "$1" == "$2" ]] || fail "expected '$2', got '$1'"; }

test_bundled_skill_matches_source() {
    "${root}/bin/sync-plugin-skill.sh" --check >/dev/null || fail "bundled skill drifted from source"
}

test_sync_detects_drift() {
    local isolated="$tmp/sync-repo"
    mkdir -p "$isolated/bin" "$isolated/.agents/skills" "$isolated/plugin/skills"
    cp "${root}/bin/sync-plugin-skill.sh" "$isolated/bin/"
    cp -R "$source_skill" "$isolated/.agents/skills/"
    cp -R "$bundled_skill" "$isolated/plugin/skills/"
    printf '\ndrift\n' >> "$isolated/plugin/skills/esdiag/SKILL.md"
    if "$isolated/bin/sync-plugin-skill.sh" --check >/dev/null 2>&1; then
        fail "drift was not detected"
    fi
}

test_skills_are_script_free() {
    [[ ! -e "${source_skill}/scripts" ]] || fail "canonical skill still contains scripts"
    [[ ! -e "${bundled_skill}/scripts" ]] || fail "generated skill still contains scripts"
    [[ ! -e "${bundled_skill}/agents" ]] || fail "generated skill contains provider metadata"
    if grep -R -F -q 'scripts/' "$source_skill"; then
        fail "canonical skill references a helper script"
    fi
}

test_skill_routes_to_native_commands() {
    grep -Fq 'esdiag init' "${source_skill}/SKILL.md" ||
        fail "canonical skill does not hand off first-run setup"
    grep -Fq 'esdiag agent skills' "${source_skill}/SKILL.md" ||
        fail "canonical skill does not document offline installation"
    grep -Fq 'esdiag agent ask' "${source_skill}/SKILL.md" ||
        fail "canonical skill does not invoke native Agent Builder ask"
    [[ -f "${source_skill}/references/onboarding.md" ]] ||
        fail "canonical skill has no onboarding reference"
}

test_host_manifests_are_valid_json() {
    jq -e . "${root}/plugin/.claude-plugin/plugin.json" >/dev/null ||
        fail "Claude plugin manifest is not valid JSON"
    jq -e . "${root}/plugin/.codex-plugin/plugin.json" >/dev/null ||
        fail "Codex plugin manifest is not valid JSON"
    jq -e . "${root}/.claude-plugin/marketplace.json" >/dev/null ||
        fail "marketplace manifest is not valid JSON"
}

test_plugin_version_matches_package_version() {
    local package_version claude_version codex_version
    package_version="$(awk -F '"' '/^version = / {print $2; exit}' "${root}/Cargo.toml")"
    package_version="${package_version%-SNAPSHOT}"
    claude_version="$(jq -r '.version' "${root}/plugin/.claude-plugin/plugin.json")"
    codex_version="$(jq -r '.version' "${root}/plugin/.codex-plugin/plugin.json")"
    assert_eq "$claude_version" "$package_version"
    assert_eq "$codex_version" "$package_version"
}

for test_name in $(declare -F | awk '{print $3}' | sort); do
    [[ "$test_name" == test_* ]] || continue
    [[ -z "$only" || "$only" == "$test_name" ]] || continue
    failed_before=$failed
    if "$test_name" && (( failed == failed_before )); then
        printf '  PASS: %s\n' "$test_name"
        passed=$((passed + 1))
    elif (( failed == failed_before )); then
        fail "$test_name returned non-zero"
    fi
done

printf '\n%s passed, %s failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
