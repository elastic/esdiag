#!/usr/bin/env bash

# Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
# or more contributor license agreements. Licensed under the Elastic License 2.0;
# you may not use this file except in compliance with the Elastic License 2.0.

set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
run_id="${ESDIAG_E2E_RUN_ID:-$(date -u +%Y%m%d%H%M%S)}"
work_dir="${ESDIAG_E2E_WORK_DIR:-${repo_root}/target/pre-release-e2e/${run_id}}"
env_file="${ESDIAG_E2E_ENV_FILE:-${repo_root}/.env.e2e}"
install_root="${work_dir}/install"
home_dir="${work_dir}/home"
output_dir="${work_dir}/output"
logs_dir="${work_dir}/logs"

es_host_name="${ESDIAG_E2E_ES_HOST:-ironhide-es}"
kibana_host_name="${ESDIAG_E2E_KIBANA_HOST:-ironhide-kibana}"
job_name="${ESDIAG_E2E_JOB_NAME:-test-job}"
keystore_password="${ESDIAG_E2E_KEYSTORE_PASSWORD:-ironhide-e2e-keystore-password}"
case_prefix="${ESDIAG_E2E_CASE_PREFIX:-ironhide-e2e-${run_id}}"

es_url="${ESDIAG_OUTPUT_URL:-http://localhost:9200}"
kibana_url="${ESDIAG_KIBANA_URL:-http://localhost:5601}"

installed_esdiag="${install_root}/bin/esdiag"
hosts_file="${home_dir}/.esdiag/hosts.yml"
keystore_file="${home_dir}/.esdiag/secrets.yml"

function log() {
    printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"
}

function fail() {
    log "ERROR: $*"
    exit 1
}

function require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "Missing required command: $1"
}

function run_step() {
    local name="$1"
    shift
    local log_file="${logs_dir}/${name}.log"
    log "START ${name}"
    if "$@" >"${log_file}" 2>&1; then
        if grep -Eq '(^|[[:space:]])ERROR([[:space:]]|:|$)|(^|[[:space:]])Error([[:space:]]|:|$)' "${log_file}"; then
            log "FAIL  ${name}; command completed but reported errors in ${log_file}"
            grep -En '(^|[[:space:]])ERROR([[:space:]]|:|$)|(^|[[:space:]])Error([[:space:]]|:|$)' "${log_file}" >&2 || true
            exit 1
        fi
        log "PASS  ${name}"
    else
        log "FAIL  ${name}; see ${log_file}"
        tail -n 80 "${log_file}" >&2 || true
        exit 1
    fi
}

function source_env_file() {
    if [[ -f "${env_file}" ]]; then
        # shellcheck disable=SC1090
        source "${env_file}"
    fi
    es_url="${ESDIAG_OUTPUT_URL:-${es_url}}"
    kibana_url="${ESDIAG_KIBANA_URL:-${kibana_url}}"
}

function es_curl() {
    local method="$1"
    local path="$2"
    shift 2

    local auth_args=()
    if [[ -n "${ESDIAG_OUTPUT_APIKEY:-}" ]]; then
        auth_args=(-H "Authorization: ApiKey ${ESDIAG_OUTPUT_APIKEY}")
    elif [[ -n "${ESDIAG_OUTPUT_USERNAME:-}" && -n "${ESDIAG_OUTPUT_PASSWORD:-}" ]]; then
        auth_args=(-u "${ESDIAG_OUTPUT_USERNAME}:${ESDIAG_OUTPUT_PASSWORD}")
    elif [[ -n "${ELASTIC_PASSWORD:-}" ]]; then
        auth_args=(-u "elastic:${ELASTIC_PASSWORD}")
    fi

    curl --fail --silent --show-error \
        --request "${method}" \
        "${auth_args[@]}" \
        -H 'Content-Type: application/json' \
        "${es_url}${path}" \
        "$@"
}

function wait_for_stack() {
    local deadline=$((SECONDS + ${ESDIAG_E2E_STACK_TIMEOUT_SECONDS:-900}))
    while (( SECONDS < deadline )); do
        if es_curl GET '/_cluster/health?wait_for_status=yellow&timeout=5s' >/dev/null 2>&1; then
            if curl --fail --silent --show-error "${kibana_url}/api/status" >/dev/null 2>&1; then
                return 0
            fi
        fi
        sleep 5
    done
    return 1
}

function wait_for_report_case() {
    local case_number="$1"
    local min_count="$2"
    local deadline=$((SECONDS + ${ESDIAG_E2E_INGEST_TIMEOUT_SECONDS:-180}))
    local body count
    body="$(jq -n --arg case_number "${case_number}" \
        '{query:{term:{"diagnostic.case_number":$case_number}}}')"

    while (( SECONDS < deadline )); do
        count="$(es_curl GET '/metrics-diagnostic-esdiag/_count' --data "${body}" | jq -r '.count // 0')" || count=0
        if [[ "${count}" =~ ^[0-9]+$ ]] && (( count >= min_count )); then
            log "Validated ${count} diagnostic report document(s) for case ${case_number}"
            return 0
        fi
        sleep 5
    done

    fail "Expected at least ${min_count} diagnostic report document(s) for case ${case_number}"
}

function run_esdiag_step() {
    local name="$1"
    shift
    run_step "${name}" \
        env \
        HOME="${home_dir}" \
        ESDIAG_HOSTS="${hosts_file}" \
        ESDIAG_KEYSTORE="${keystore_file}" \
        ESDIAG_KEYSTORE_PASSWORD="${keystore_password}" \
        "${installed_esdiag}" \
        "$@"
}

function cleanup_existing_state() {
    if [[ "${ESDIAG_E2E_CLEAN_REMOTE:-true}" != "true" ]]; then
        return
    fi
    local delete_query
    delete_query="$(jq -n --arg prefix "${case_prefix}" \
        '{query:{prefix:{"diagnostic.case_number":$prefix}}}')"
    es_curl POST '/metrics-diagnostic-esdiag/_delete_by_query?conflicts=proceed&refresh=true&ignore_unavailable=true' \
        --data "${delete_query}" >/dev/null 2>&1 || true
}

function main() {
    cd "${repo_root}"
    require_command cargo
    require_command curl
    require_command jq

    mkdir -p \
        "${install_root}" \
        "${home_dir}/.esdiag" \
        "${output_dir}/collect-es" \
        "${output_dir}/collect-kibana" \
        "${logs_dir}"
    if [[ ! -f "${env_file}" ]]; then
        cp "${repo_root}/example.env" "${env_file}"
        chmod 600 "${env_file}"
    fi

    log "Work directory: ${work_dir}"
    run_step stack-up ./bin/esdiag-control up --env "${env_file}" -b false
    source_env_file
    wait_for_stack || fail "Elastic Stack did not become healthy in time"
    run_step stack-auth ./bin/esdiag-control auth --env "${env_file}"
    source_env_file

    cleanup_existing_state

    run_step cargo-install cargo install --path "${repo_root}" --root "${install_root}" --force
    run_step esdiag-version "${installed_esdiag}" --version

    [[ -n "${ELASTIC_PASSWORD:-}" ]] || fail "ELASTIC_PASSWORD was not found after sourcing ${env_file}"

    run_esdiag_step keystore-create keystore add ironhide-es-basic --user elastic --password "${ELASTIC_PASSWORD}"
    run_esdiag_step keystore-add-kibana keystore add ironhide-kibana-basic --user elastic --password "${ELASTIC_PASSWORD}"

    run_esdiag_step host-add-es host add "${es_host_name}" elasticsearch "${es_url}" --secret ironhide-es-basic --roles collect,send
    run_esdiag_step host-add-kibana host add "${kibana_host_name}" kibana "${kibana_url}" --secret ironhide-kibana-basic --roles collect,view
    run_esdiag_step host-auth-es host auth "${es_host_name}"
    run_esdiag_step host-auth-kibana host auth "${kibana_host_name}"

    run_esdiag_step collect-es collect "${es_host_name}" "${output_dir}/collect-es" --case "${case_prefix}-collect-es" --user ironhide-e2e
    run_esdiag_step collect-kibana collect "${kibana_host_name}" "${output_dir}/collect-kibana" --case "${case_prefix}-collect-kibana" --user ironhide-e2e

    run_esdiag_step process-es process "${es_host_name}" "${es_host_name}" --case "${case_prefix}-process-es" --user ironhide-e2e
    wait_for_report_case "${case_prefix}-process-es" 1

    if [[ "${ESDIAG_E2E_PROCESS_KIBANA:-false}" == "true" ]]; then
        run_esdiag_step process-kibana process "${kibana_host_name}" "${es_host_name}" --case "${case_prefix}-process-kibana" --user ironhide-e2e
        wait_for_report_case "${case_prefix}-process-kibana" 1
    else
        log "SKIP  process-kibana: Kibana processing is not implemented in this build"
    fi

    run_esdiag_step save-compound-job process "${es_host_name}" "${es_host_name}" --save-job "${job_name}" --case "${case_prefix}-compound-save" --user ironhide-e2e
    wait_for_report_case "${case_prefix}-compound-save" 1

    run_esdiag_step job-list job list
    run_esdiag_step job-run job run "${job_name}"
    wait_for_report_case "${case_prefix}-compound-save" 2

    log "Pre-release E2E suite passed without errors"
    log "Logs: ${logs_dir}"
}

main "$@"
