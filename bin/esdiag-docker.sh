#!/bin/bash

# Export these variables, they will be passed to the container
declare ESDIAG_OUTPUT_URL=${ESDIAG_OUTPUT_URL:-"http://host.docker.internal:9200"}
declare ESDIAG_OUTPUT_APIKEY=${ESDIAG_OUTPUT_APIKEY}
declare ESDIAG_OUTPUT_USERNAME=${ESDIAG_OUTPUT_USERNAME}
declare ESDIAG_OUTPUT_PASSWORD=${ESDIAG_OUTPUT_PASSWORD}

# ----- Logging Functions -----

declare log_name="esdiag-docker"

# Colorized echo statements
function blue()    { echo -e -n "\033[94m${1}\033[39m"; }
function cyan()    { echo -e -n "\033[36m${1}\033[39m"; }
function gray()    { echo -e -n "\033[90m${1}\033[39m"; }
function green()   { echo -e -n "\033[32m${1}\033[39m"; }
function magenta() { echo -e -n "\033[35m${1}\033[39m"; }
function red()     { echo -e -n "\033[31m${1}\033[39m"; }
function white()   { echo -e -n "\033[97m${1}\033[39m"; }
function yellow()  { echo -e -n "\033[33m${1}\033[39m"; }

# Colorized log messages
function timestamp() { echo -n $(date -u +"%Y-%m-%d %H:%M:%S"); }
function log_error() { echo "[$(timestamp) $(red Error) ${log_name}] ${1}"; }
function log_warn()  { echo "[$(timestamp) $(yellow Warn) ${log_name}] ${1}"; }
function log_info()  { echo "[$(timestamp) $(green Info) ${log_name}] ${1}"; }
function log_debug() {
    if [[ $LOG_LEVEL == "debug" ]]; then
        echo "[$(timestamp) $(blue Debug) ${log_name}] ${1}"
    fi
}
# ----- Main Functions -----

function dependencies_validate() {
    local failures=0
    if ! command -v docker &> /dev/null; then
        log_error "$(white docker) is required to build and run Docker containers"
        failures=$((failures + 1))
    fi

    if ! command -v jq &> /dev/null; then
        log_error "$(white jq) is required to validate the container image"
        log_error "Check your linux distribution package manager or MacOS homebrew"
        failures=$((failures + 1))
    fi

    if (( $failures > 0 )); then
        log_error "Dependencies: $(white $failures) checks $(red failed)"
        exit 1
    fi
}

function container_image_validate() {
    # call docker inspect esdiag:latest, with jq check that .[].RepoTags[0] == "esdiag:latest"
    local is_valid=$(docker inspect esdiag:latest | jq '.[].RepoTags | contains(["esdiag:latest"])')
    if [[ "${is_valid}" != "true" ]]; then
        log_error "Container image $(red "not found") with tag $(gray esdiag:latest)"
        log_error "Please run $(white "docker build --tag esdiag:latest .")"
        exit 1
    else
        log_info "Container image $(green found) with tag $(cyan esdiag:latest)"
    fi
}

# If diag_path is a local file or directory, mount it to the container
function docker_run() {
    declare input="${1}"; shift
    if [[ -f "${input}" ]] || [[ -d "${input}" ]]; then
        log_info "Path $(gray ${input}) is local file or directory, mounting to container"
        declare diag_mount="/data/diagnostic"
    fi

    log_info "Running $(white "esdiag ${command} ${input} ${*}")"

    docker run --rm ${diag_mount:+--volume ${input}:${diag_mount}} \
        --env ESDIAG_OUTPUT_URL="${ESDIAG_OUTPUT_URL}" \
        ${ESDIAG_OUTPUT_APIKEY:+--env ESDIAG_OUTPUT_APIKEY="${ESDIAG_OUTPUT_APIKEY}"} \
        ${ESDIAG_OUTPUT_USERNAME:+--env ESDIAG_OUTPUT_USERNAME="${ESDIAG_OUTPUT_USERNAME}"} \
        ${ESDIAG_OUTPUT_PASSWORD:+--env ESDIAG_OUTPUT_PASSWORD="${ESDIAG_OUTPUT_PASSWORD}"} \
        esdiag:latest "${command}" ${diag_mount:-${input}} ${*}
}

# ----- Main -----

dependencies_validate && container_image_validate
declare command="${1}"; shift
case "${command}" in
    "collect")
        log_warn "The $(white collect) command is not supported from this Docker container"
        exit 1
        ;;
    "host")
        log_warn "The $(white host) command is not supported from this Docker container."
        echo "Instead, configure these environment variables that will pass through:"
        echo "    - ESDIAG_OUTPUT_URL"
        echo "    - ESDIAG_OUTPUT_APIKEY"
        echo "    - ESDIAG_OUTPUT_USERNAME"
        echo "    - ESDIAG_OUTPUT_PASSWORD"
        echo
        echo " The $(gray URL) is required with either the $(gray APIKEY) or $(gray USERNAME) and $(gray PASSWORD)."
        exit 1
        ;;
    *)
        docker_run ${*}
        ;;
esac
