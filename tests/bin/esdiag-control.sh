#!/bin/bash

# ----- Environment -----

declare esdiag_dir="${1:-.}"
declare version=$(grep -o '^version = ".*"' Cargo.toml | sed -E 's/^version = "(.*)"/\1/')
declare tests_passed=0
declare tests_failed=0
declare tests_total=0
declare test_log="target/test-esdiag-control.log"
truncate -s 0 ${test_log}

# ----- Logging -----

declare log_name="test-esdiag-control"

# Only print colors if output is a terminal
declare colorize=false
if [[ -t 1 ]]; then
    colorize=true
fi

function echo_color() {
    local color=$1; shift
    if [[ $colorize == true ]]; then
        echo -e -n "\033[${color}m${*}\033[39m"
    else
        echo -n "${*}"
    fi
}

# Colorized echo statements
function black()     { echo_color 30 "${@}"; }
function red()       { echo_color 31 "${@}"; }
function green()     { echo_color 32 "${@}"; }
function yellow()    { echo_color 33 "${@}"; }
function blue()      { echo_color 34 "${@}"; }
function magenta()   { echo_color 35 "${@}"; }
function cyan()      { echo_color 36 "${@}"; }
function gray()      { echo_color 90 "${@}"; }
function lt_red()    { echo_color 91 "${@}"; }
function lt_green()  { echo_color 92 "${@}"; }
function lt_yellow() { echo_color 93 "${@}"; }
function lt_blue()   { echo_color 94 "${@}"; }
function lt_magenta(){ echo_color 95 "${@}"; }
function lt_cyan()   { echo_color 96 "${@}"; }
function white()     { echo_color 97 "${@}"; }

# Colorized log messages
function timestamp() { echo -n "$(date -u +"%Y-%m-%d %H:%M:%S")"; }
function log_error() { echo "[$(timestamp) $(red Error) ${log_name}] ${*}"; }
function log_warn()  { echo "[$(timestamp) $(yellow Warn)  ${log_name}] ${*}"; }
function log_info()  { echo "[$(timestamp) $(green Info)  ${log_name}] ${*}"; }
function log_debug() {
    if [[ $LOG_LEVEL == "debug" ]]; then
        echo "[$(timestamp) $(blue Debug) ${log_name}] ${*}"
    fi
}

# ----- Utility -----

function test_start() {
    log_info "$(cyan start) ${*}" >> ${test_log}
    tests_total=$((tests_total + 1))
}

function test_fail() {
    log_error "$(red failed) ${*}"
    log_error "$(red failed) ${*}" >> ${test_log}
    tests_failed=$((tests_failed + 1))
}

function test_pass() {
    log_info "$(green passed) ${*}"
    log_info "$(green passed) ${*}" >> ${test_log}
    tests_passed=$((tests_passed + 1))
}

function esdiag_control() {
    local command=$1; shift
    ${esdiag_dir}/bin/esdiag-control $command --env .env.test ${*} >> ${test_log} 2>&1
}

# ----- Tests -----

function shellcheck_returns_zero_issues() {
    test_start "shellcheck_returns_zero_issues"
    lines=$(shellcheck "${esdiag_dir}/bin/esdiag-control" | wc -l)
    if (( lines == 0 )); then
        test_pass shellcheck_returns_zero_issues
    else
        test_fail shellcheck_returns_zero_issues returned ${lines} lines
    fi
}

function command_help_prints_usage() {
    test_start "command_help_prints_usage"
    esdiag_control help
    lines=$(grep "Usage:" ${test_log} | wc -l)
    if (( lines > 0 )); then
        test_pass command_help_prints_usage
    else
        test_fail command_help_prints_usage with exit code "${?}"
    fi
}

function command_build_creates_container_image() {
    test_start "command_build_creates_container_image"
    local image_id && image_id=$(${container} image ls --quiet esdiag:${version})

    # make sure the esdiag:version image doesn't exist
    if [[ -n ${image_id} ]]; then
        log_debug "Removing existing image ${image_id}"
        ${container} rmi ${image_id} >> ${test_log} 2>&1 \
        || log_warn "$(yellow failed) to remove existing image ${image_id}"
    fi

    # Build the esdiag image
    if esdiag_control build; then
        image_id=$(${container} image ls --quiet esdiag:${version})
        test_pass command_build_creates_container_image
    else
        test_fail command_build_creates_container_image with exit code "${?}"
    fi
}

function command_buildx_creates_multi_platform_images() {
    test_start "command_buildx_creates_multi_platform_images"
    local image_id && image_id=$(${container} image ls --quiet esdiag:${version})

    # make sure the esdiag:version image doesn't exist
    if [[ -n ${image_id} ]]; then
        log_debug "Removing existing image ${image_id}"
        ${container} rmi ${image_id} >> ${test_log} 2>&1 \
        || log_error "$(red failed) to remove existing image ${image_id}"
    fi

    # Build the esdiag image
    if esdiag_control buildx >> ${test_log} 2>&1; then
        image_id=$(${container} image ls --quiet esdiag:${version})
        test_pass command_buildx_creates_multi_platform_images "${image_id}"
    else
        test_fail command_buildx_creates_multi_platform_images with exit code "${?}"
    fi
}

function command_auth_returns_success() {
    test_start "command_auth_returns_success"
    esdiag_control auth
    elasticsearch_auth=$(grep "esdiag-control.*You Know, for Search" ${test_log} | wc -l)
    kibana_auth=$(grep "esdiag-control] Kibana space" target/test-esdiag-control.log | grep -v "failed" | wc -l)
    log_debug "Elasticsearch auth: $(white "${elasticsearch_auth}") Kibana auth: $(white "${kibana_auth}")"

    if [[ ${elasticsearch_auth} -eq 1 && ${kibana_auth} -eq 1 ]]; then
        test_pass command_auth_returns_success
    else
        test_fail command_auth_returns_success
    fi
}

function command_launch_insecure_starts_stack_containers() {
    test_start "command_launch_insecure_starts_stack_containers"
    esdiag_control launch --insecure

    elasticsearch_status=$(${container} inspect esdiag-elasticsearch --format '{{.State.Status}}')
    kibana_status=$(${container} inspect esdiag-kibana --format '{{.State.Status}}')
    esdiag_status=$(${container} inspect esdiag --format '{{.State.Status}}')

    if [[ ${elasticsearch_status} == "running" && ${kibana_status} == "running" && ${esdiag_status} == "running" ]]; then
        test_pass command_launch_insecure_starts_stack_containers
    else
        test_fail command_launch_insecure_starts_stack_containers
    fi
}

function command_setup_completes_successfully() {
    test_start "command_setup_completes_successfully"
    esdiag_control setup
    success=$(tail -n 20 ${test_log} | grep "esdiag-control].*esdiag setup.*is.*complete.*!$" | wc -l)

    log_debug "Setup success: $(white "${success}")"
    if [[ ${success} -eq 1 ]]; then
        test_pass command_setup_completes_successfully
    else
        test_fail command_setup_completes_successfully
    fi
}

function command_remove_removes_containers {
    test_start "command_remove_removes_containers"
    esdiag_control remove
    containers=$("$container" ps -a | grep esdiag | wc -l)

    log_debug "Remove containers: $(white "${containers}")"
    if [[ ${containers} -eq 0 ]]; then
        test_pass command_remove_removes_containers
    else
        test_fail command_remove_removes_containers found $(magenta "${containers}") $(gray esdiag-*) containers
    fi
}

function command_launch_secure_starts_stack_containers {
    test_start "command_launch_secure_starts_stack_containers"
    sed -i -e 's/ELASTIC_SECURITY_ENABLED=false/ELASTIC_SECURITY_ENABLED=true/' .env.test
    esdiag_control launch

    elasticsearch_status=$(${container} inspect esdiag-elasticsearch --format '{{.State.Status}}')
    kibana_status=$(${container} inspect esdiag-kibana --format '{{.State.Status}}')
    esdiag_status=$(${container} inspect esdiag --format '{{.State.Status}}')

    if [[ ${elasticsearch_status} == "running" && ${kibana_status} == "running" && ${esdiag_status} == "running" ]]; then
        test_pass command_launch_secure_starts_stack_containers
    else
        test_fail command_launch_secure_starts_stack_containers Elasticsearch: $(magenta "${elasticsearch_status}") Kibana: $(magenta "${kibana_status}") ESDiag: $(magenta "${esdiag_status}")
    fi
}

# ----- Main -----

function env_setup() {
    if [[ -f .env ]]; then
        cp .env .env.test
    else
        cp example.env .env.test
    fi
    chmod 600 .env.test

    if ! command -v shellcheck &> /dev/null; then
        log_error "$(magenta shellcheck) is required for testing"
        exit 1
    fi

    if command -v podman &> /dev/null; then
        container="podman"
    elif command -v docker &> /dev/null; then
        container="docker"
    else
        log_error "Required container runtime $(magenta docker) or $(magenta podman) is not found"
        exit 1
    fi
    export container
}

function tests_summary() {
    if (( tests_failed > 0 )); then
        log_info "Tests run: $(cyan "${tests_total}") passed: $(green "${tests_passed}") failed: $(red "${tests_failed}")"
    else
        log_info "Tests run: $(cyan "${tests_total}") passed: $(green "${tests_passed}") failed: $(green "${tests_failed}")"
    fi
}

function tests_run() {
    shellcheck_returns_zero_issues
    command_help_prints_usage
    command_build_creates_container_image
    # First launch and auth with security disabled
    command_launch_insecure_starts_stack_containers
    command_auth_returns_success
    command_setup_completes_successfully
    command_remove_removes_containers
    # Second launch and with security enabled
    command_launch_secure_starts_stack_containers
    command_auth_returns_success
}

env_setup
tests_run
tests_summary
