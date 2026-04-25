#!/bin/bash

# Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
# or more contributor license agreements. Licensed under the Elastic License 2.0;
# you may not use this file except in compliance with the Elastic License 2.0.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARCHIVE_DIR="${ROOT_DIR}/tests/archives"
ESDIAG_BIN="${ROOT_DIR}/target/debug/esdiag"
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-docker}"
KIBANA_ENCRYPTION_KEY="${KIBANA_ENCRYPTION_KEY:-0123456789abcdef0123456789abcdef}"
LOGSTASH_PIPELINE_DIR="$(mktemp -d)"

VERSIONS=(
    "6.8.23"
    "7.17.29"
    "8.19.3"
    "9.3.3"
)
SELECTED_VERSIONS=()

cleanup() {
    local version kebab
    for version in "${VERSIONS[@]}"; do
        kebab="${version//./-}"
        "${CONTAINER_RUNTIME}" rm -f "esdiag-es-${kebab}" "esdiag-kb-${kebab}" "esdiag-ls-${kebab}" >/dev/null 2>&1 || true
        "${CONTAINER_RUNTIME}" network rm "esdiag-fixtures-${kebab}" >/dev/null 2>&1 || true
    done
    rm -rf "${LOGSTASH_PIPELINE_DIR}"
}
trap cleanup EXIT

log() {
    printf '[regen-fixtures] %s\n' "$*"
}

version_is_supported() {
    local candidate="$1"
    local version
    for version in "${VERSIONS[@]}"; do
        if [[ "${version}" == "${candidate}" ]]; then
            return 0
        fi
    done
    return 1
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'Missing required command: %s\n' "$1" >&2
        exit 1
    fi
}

wait_for_http() {
    local url="$1"
    local attempts="$2"
    local delay="$3"
    local attempt
    for attempt in $(seq 1 "${attempts}"); do
        if curl -fsS "${url}" >/dev/null 2>&1; then
            return 0
        fi
        sleep "${delay}"
    done
    return 1
}

docker_host_port() {
    local container="$1"
    local port="$2"
    "${CONTAINER_RUNTIME}" inspect -f "{{(index (index .NetworkSettings.Ports \"${port}/tcp\") 0).HostPort}}" "${container}"
}

build_esdiag_if_needed() {
    if [[ ! -x "${ESDIAG_BIN}" ]]; then
        log "Building esdiag binary"
        cargo build --manifest-path "${ROOT_DIR}/Cargo.toml"
    fi
}

collect_fixture() {
    local host_name="$1"
    local product="$2"
    local url="$3"
    local fixture_path="$4"
    local home_dir out_dir zip_files

    home_dir="$(mktemp -d)"
    out_dir="$(mktemp -d)"

    HOME="${home_dir}" USERPROFILE="${home_dir}" "${ESDIAG_BIN}" host add "${host_name}" "${product}" "${url}" --agent >/dev/null
    HOME="${home_dir}" USERPROFILE="${home_dir}" "${ESDIAG_BIN}" collect "${host_name}" "${out_dir}" --type support --agent >/dev/null

    shopt -s nullglob
    zip_files=("${out_dir}"/*.zip)
    shopt -u nullglob
    if (( ${#zip_files[@]} != 1 )); then
        printf 'Expected exactly one archive in %s, found %s\n' "${out_dir}" "${#zip_files[@]}" >&2
        exit 1
    fi

    cp "${zip_files[0]}" "${fixture_path}"
    rm -rf "${home_dir}" "${out_dir}"
}

start_elasticsearch() {
    local version="$1"
    local kebab="${version//./-}"
    local container="esdiag-es-${kebab}"
    local network="esdiag-fixtures-${kebab}"
    local port
    local env_args=(
        -e discovery.type=single-node
        -e ES_JAVA_OPTS='-Xms512m -Xmx512m'
        -e xpack.security.enabled=false
    )

    "${CONTAINER_RUNTIME}" rm -f "${container}" >/dev/null 2>&1 || true
    "${CONTAINER_RUNTIME}" network rm "${network}" >/dev/null 2>&1 || true
    "${CONTAINER_RUNTIME}" network create "${network}" >/dev/null

    if [[ "${version}" != 6.8.* && "${version}" != 7.17.* ]]; then
        env_args+=(-e xpack.security.enrollment.enabled=false)
    fi

    "${CONTAINER_RUNTIME}" run -d \
        --name "${container}" \
        --network "${network}" \
        --network-alias elasticsearch \
        -p 127.0.0.1::9200 \
        "${env_args[@]}" \
        "docker.elastic.co/elasticsearch/elasticsearch:${version}" >/dev/null

    port="$(docker_host_port "${container}" 9200)"
    if ! wait_for_http "http://127.0.0.1:${port}/" 180 2; then
        "${CONTAINER_RUNTIME}" logs "${container}" >&2 || true
        printf 'Elasticsearch %s did not become ready\n' "${version}" >&2
        exit 1
    fi

    printf '%s\n' "${port}"
}

start_kibana() {
    local version="$1"
    local kebab="${version//./-}"
    local container="esdiag-kb-${kebab}"
    local network="esdiag-fixtures-${kebab}"
    local port
    local env_args=()

    "${CONTAINER_RUNTIME}" rm -f "${container}" >/dev/null 2>&1 || true

    if [[ "${version}" == 6.8.* ]]; then
        env_args+=(-e ELASTICSEARCH_URL='http://elasticsearch:9200')
    else
        env_args+=(-e 'ELASTICSEARCH_HOSTS=["http://elasticsearch:9200"]')
        env_args+=(-e "XPACK_ENCRYPTEDSAVEDOBJECTS_ENCRYPTIONKEY=${KIBANA_ENCRYPTION_KEY}")
        env_args+=(-e "XPACK_REPORTING_ENCRYPTIONKEY=${KIBANA_ENCRYPTION_KEY}")
        env_args+=(-e "XPACK_SECURITY_ENCRYPTIONKEY=${KIBANA_ENCRYPTION_KEY}")
    fi
    env_args+=(-e XPACK_SECURITY_ENABLED=false)
    env_args+=(-e "SERVER_NAME=esdiag-kibana-${kebab}")

    "${CONTAINER_RUNTIME}" run -d \
        --name "${container}" \
        --network "${network}" \
        -p 127.0.0.1::5601 \
        "${env_args[@]}" \
        "docker.elastic.co/kibana/kibana:${version}" >/dev/null

    port="$(docker_host_port "${container}" 5601)"
    if ! wait_for_http "http://127.0.0.1:${port}/api/status" 240 2; then
        "${CONTAINER_RUNTIME}" logs "${container}" >&2 || true
        printf 'Kibana %s did not become ready\n' "${version}" >&2
        exit 1
    fi

    printf '%s\n' "${port}"
}

start_logstash() {
    local version="$1"
    local kebab="${version//./-}"
    local container="esdiag-ls-${kebab}"
    local port

    "${CONTAINER_RUNTIME}" rm -f "${container}" >/dev/null 2>&1 || true
    "${CONTAINER_RUNTIME}" run -d \
        --name "${container}" \
        -p 127.0.0.1::9600 \
        -e LS_JAVA_OPTS='-Xms256m -Xmx256m' \
        -v "${LOGSTASH_PIPELINE_DIR}/logstash.conf:/usr/share/logstash/pipeline/logstash.conf:ro" \
        "docker.elastic.co/logstash/logstash:${version}" >/dev/null

    port="$(docker_host_port "${container}" 9600)"
    if ! wait_for_http "http://127.0.0.1:${port}/" 180 2; then
        "${CONTAINER_RUNTIME}" logs "${container}" >&2 || true
        printf 'Logstash %s did not become ready\n' "${version}" >&2
        exit 1
    fi

    printf '%s\n' "${port}"
}

regenerate_version() {
    local version="$1"
    local kebab="${version//./-}"
    local es_port kb_port ls_port
    local es_pid kb_pid ls_pid
    local status=0

    log "Refreshing ${version}"

    rm -f \
        "${ARCHIVE_DIR}/elasticsearch-api-diagnostics-${version}.zip" \
        "${ARCHIVE_DIR}/kibana-api-diagnostics-${version}.zip" \
        "${ARCHIVE_DIR}/logstash-api-diagnostics-${version}.zip"

    es_port="$(start_elasticsearch "${version}")"
    kb_port="$(start_kibana "${version}")"
    ls_port="$(start_logstash "${version}")"

    # Once the services are ready, each fixture collection is independent.
    collect_fixture "es-${kebab}" "elasticsearch" "http://127.0.0.1:${es_port}" \
        "${ARCHIVE_DIR}/elasticsearch-api-diagnostics-${version}.zip" &
    es_pid=$!
    collect_fixture "kb-${kebab}" "kibana" "http://127.0.0.1:${kb_port}" \
        "${ARCHIVE_DIR}/kibana-api-diagnostics-${version}.zip" &
    kb_pid=$!
    collect_fixture "ls-${kebab}" "logstash" "http://127.0.0.1:${ls_port}" \
        "${ARCHIVE_DIR}/logstash-api-diagnostics-${version}.zip" &
    ls_pid=$!

    wait "${es_pid}" || status=$?
    wait "${kb_pid}" || status=$?
    wait "${ls_pid}" || status=$?
    if (( status != 0 )); then
        printf 'Fixture collection failed for %s\n' "${version}" >&2
        exit "${status}"
    fi

    "${CONTAINER_RUNTIME}" rm -f "esdiag-es-${kebab}" "esdiag-kb-${kebab}" "esdiag-ls-${kebab}" >/dev/null
    "${CONTAINER_RUNTIME}" network rm "esdiag-fixtures-${kebab}" >/dev/null
}

main() {
    local version

    require_command "${CONTAINER_RUNTIME}"
    require_command curl
    require_command cargo

    if [[ ! -d "${ARCHIVE_DIR}" ]]; then
        printf 'Archive directory does not exist: %s\n' "${ARCHIVE_DIR}" >&2
        exit 1
    fi

    printf 'input { heartbeat { interval => 60 message => "esdiag" } }\noutput { stdout { codec => json } }\n' \
        > "${LOGSTASH_PIPELINE_DIR}/logstash.conf"

    build_esdiag_if_needed
    rm -rf "${ARCHIVE_DIR}/tmp"

    if (( $# > 0 )); then
        for version in "$@"; do
            if ! version_is_supported "${version}"; then
                printf 'Unsupported version: %s\n' "${version}" >&2
                printf 'Supported versions: %s\n' "${VERSIONS[*]}" >&2
                exit 1
            fi
            SELECTED_VERSIONS+=("${version}")
        done
    else
        SELECTED_VERSIONS=("${VERSIONS[@]}")
    fi

    for version in "${SELECTED_VERSIONS[@]}"; do
        regenerate_version "${version}"
    done

    log "Fixture regeneration complete"
}

main "$@"
