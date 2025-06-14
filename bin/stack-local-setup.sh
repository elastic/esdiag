#!/bin/bash

# Setup a complete, local, Docker container-powered Elastic Stack for ESDiag.
# 1. Builds an ESDiag container image for use with `esdiag-docker.sh`
# 2. Starts a security-disabled Kibana and Elasticsearch via `docker compose`
# 3. Sets up Elasticsearch templates via `esdiag setup`
# 4. Imports Kibana saved objects from an `.ndjson` file
# 5. Opens Kibana in your web browser

# ----- User Configuration -----

declare kibana_url="http://localhost:5601"
declare elasticsearch_url="http://localhost:9200"

# ----- Advanced Configuration -----

# Use ESDIAG_OUTPUT* environment variables to configure authentication

declare apikey=${ESDIAG_OUTPUT_APIKEY}
declare username=${ESDIAG_OUTPUT_USERNAME}
declare password=${ESDIAG_OUTPUT_PASSWORD}

# Landing page when opening the web browser
declare kibana_homepage="/app/dashboards#/view/2c8cd284-79ef-4787-8b79-0030e0df467b"

# The `export.ndjson` file to import into Kibana, provided as the first argument to the script
# Defaults to the newest dashboard file in `assets/kibana/esdiag-dashboards*.ndjson`
declare dashboard_file=${1:-$(ls -tr assets/kibana/esdiag-dashboards*.ndjson 2>/dev/null | tail -n 1)}

# URL to download the latest dashboards release file
declare esdiag_dashboards_url="https://github.com/elastic/esdiag-dashboards/releases/latest"

# ----- Logging Functions -----

declare log_name="stack-local-setup"

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

# ----- Kibana Functions -----

# Build Kibana authorization header
function set_auth_header() {
    log_debug "Setting Kibana auth header"
    # Exit if Kibana URL was not defined
    if [[ -z $kibana_url ]]; then
        log_error "Kibana URL is not defined"
        exit 1
    fi

    if [[ ! -z $apikey ]]; then
        log_info "Using apikey authorization to: $(blue "${kibana_url}")"
        export auth_header="Authorization: ApiKey ${kibana_apikey}"
    elif [[ ! -z $password ]]; then
        log_info "Using basic authorization for ${username} to: $(blue "${kibana_url}")"
        export auth_header="Authorization: Basic $(echo -n "${username}:${password}" | base64)"
    else
        log_info "Using no authorization to: $(blue "${kibana_url}")"
        export auth_header="Authorization: None"
    fi
}

function kibana_objects_import() {
    local kibana_space="default"
    local response_file="target/dashboard_import.json"

    # Don't import before Kibana is responding

    local http_status=0
    while [[ $http_status -ne 200 ]]; do
        http_status=$(curl --write-out "%{http_code}" --silent --output /dev/null "${kibana_url}/app/home")
        if [[ $http_status -ne 200 ]]; then
            log_info "Waiting on $(magenta Kibana)..."
            sleep 5
        fi
    done
    log_info "$(magenta Kibana) is $(green ready)!"

    # Import saved objects

    set_auth_header
    log_info "Importing $(gray $dashboard_file) to $(blue ${kibana_url}) in the $(gray ${kibana_space}) space"
    curl -X POST "${kibana_url}/s/${kibana_space}/api/saved_objects/_import?overwrite=true" \
        --header "${auth_header}" \
        --header "kbn-xsrf: true" \
        --compressed \
        --silent \
        --form file=@${dashboard_file} \
        | jq > "${response_file}"

    local success=$(jq -r .success "${response_file}")
    if [[ $success != "true" ]]; then
        log_error "Import failed, check $(gray "${response_file}") for details"
        exit 1
    fi
    export success_count=$(jq -r .successCount "${response_file}")
}

function browser_homepage_open() {
    local homepage_url="${kibana_url}${kibana_homepage}"
    log_info "Opening web browser to $(blue "${homepage_url}")"
    open ${homepage_url}
}

# ----- Container Functions -----

function stack_containers_run() {
    log_info "Running $(white "docker compose up -d") in background"
    docker compose --file docker/docker-compose.yml up --detach > /dev/null 2>&1 &
    wait $!
    if [[ $? -ne 0 ]]; then
        log_error "$(white "docker compose up") $(red failed) with exit status ${?}"
        exit $?
    fi
}

function containers_build_and_run() {
    stack_containers_run &
    local esdiag_version=$(grep -o '^version = ".*"' Cargo.toml | sed -E 's/^version = "(.*)"/\1/')
    if [[ $(docker images -q esdiag:${esdiag_version} 2> /dev/null) == "" ]]; then
        log_info "Building $(cyan "esdiag:${esdiag_version}") container image"
    else
        log_info "Found container image $(cyan "esdiag:${esdiag_version}"), skipping build"
        return
    fi

    docker build --tag esdiag:${esdiag_version} .
    if [[ $? -eq 0 ]]; then
        log_info "$(white "docker build") completed $(green successfully)"
        docker tag esdiag:${esdiag_version} esdiag:latest
    else
        log_error "$(white "docker build") $(red failed) with exit status ${?}"
        exit $?
    fi
}

# ----- Elasticsearch Functions -----

function elasticsearch_templates_setup() {
    local http_status=0
    while [[ $http_status -ne 200 ]]; do
        http_status=$(curl --write-out "%{http_code}" --silent --output /dev/null "${elasticsearch_url}/")
        if [[ $http_status -ne 200 ]]; then
            log_info "Waiting on $(blue Elasticsearch)..."
            sleep 5
        fi
    done
    log_info "$(blue Elasticsearch) is $(green ready)!"

    bin/esdiag-docker.sh setup

    if [[ $? -eq 0 ]]; then
        log_info "$(white "esdiag setup") completed $(green "successfully")"
    else
        log_error "$(cyan ESDiag) setup $(red failed) with exit status ${?}"
        exit $?
    fi
}

# ----- Main Functions -----

function dependencies_validate() {
    if [[ ! -f "Cargo.toml" ]]; then
        log_error "$(gray Cargo.toml) file not found in the current directory"
        echo "Please run this script from the $(gray root) of the $(cyan esdiag) repository:"
        echo "  $(white "./bin/stack-local-setup.sh")"
        exit 1
    fi

    local failures=0

    if ! command -v docker &> /dev/null; then
        log_error "$(white docker) is required to build and run conatiners"
        failures=$((failures + 1))
    fi

    if ! command -v curl &> /dev/null; then
        log_error "$(white curl) is required to send HTTP requests"
        failures=$((failures + 1))
    fi

    if ! command -v jq &> /dev/null; then
        log_error "$(white jq) is required to read from json files and $(white curl) responses"
        failures=$((failures + 1))
    fi

    if ! command -v grep &> /dev/null; then
        log_error "$(white grep) is required to search files"
        failures=$((failures + 1))
    fi

    if ! command -v sed &> /dev/null; then
        log_error "$(white sed) is required to process text"
        failures=$((failures + 1))
    fi

    if [[ ! -f "bin/esdiag-docker.sh" ]]; then
        log_error "$(white bin/esdiag-docker.sh) script not found from the current directory"
        failures=$((failures + 1))
    fi

    if [[ ! -f $dashboard_file ]]; then
        log_error "Dashboard file $(gray "${dashboard_file}") does not exist"
        echo "ESDiag $(white "${0}") requires a dashboards import file. Download the latest $(gray .ndjson) file from:"
        echo "  $(blue "${esdiag_dashboards_url}")"
        echo "Then provide a valid dashboard file path as an argument:"
        echo "  $(gray "./bin/stack-local-setup.sh ~/Downloads/esdiag-dashboards_2025-04-20.ndjson")"
        failures=$((failures + 1))
    fi

    if (( $failures > 0 )); then
        log_error "Dependencies: $(white $failures) checks $(red failed)"
        exit 1
    fi
}

# ----- Main -----

dependencies_validate \
&& containers_build_and_run \
&& elasticsearch_templates_setup \
&& kibana_objects_import \
&& browser_homepage_open
