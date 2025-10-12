#!/bin/bash

# ---------- Description ----------

# The `min-diag.sh` is a script to `collect` the minimum Elasticsearch diagnostic
# bundles with a `watch` function to periodically collect at regular intervals.
#
# The `collect` command pulls one minimal diagnostic bundle from the cluster:
#
# ```bash
# ./min-diag.sh collect
# ```
#
# Outputs one directory named `api-diagnostics-<timestamp>` with the diagnostic files in it.
#
# The `watch` command periodically collects diagnostic bundles from the cluster:
#
# ```bash
# ./min-diag.sh watch
# ```
#
# Outputs many directories named `api-diagnostics-<timestamp>`.
#
# Only modify the variables in the `Configuration` section for your environment.

# ---------- Configuration ----------

# Use the encoded Elasticsearch API key for cluster authentication
declare APIKEY=""

# Elasticsearch cluster URL, no trailing slashes
declare URL=""

# Elastic Cloud Admin deployment ID and API proxy url
#declare DEPLOYMENT_ID=""
#declare ADMIN_DOMAIN=""
#declare URL="https://${ADMIN_DOMAIN}/api/v1/deployments/${DEPLOYMENT_ID}/elasticsearch/main-elasticsearch/proxy"

# Seconds between each collection start, be sure to wait long enough not to overlap collections
declare WAIT_SECONDS=60
# Number of collections to perform, for example a 60 second wait * 60 collections = 30 minute runtime
declare COLLECTION_COUNT=5

# ------ Logging Functions ------

declare log_name="min-diag"

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

# ---------- Funcitons ----------

# Prints script usage information
function help() {
    white "Usage: $(green "$0" "<COMMAND>")"
    echo
    white "Commands:"
    echo "  $(green watch)   - Collect diagnostics periodically based on the configured WAIT_SECONDS and COLLECTION_COUNT."
    echo "  $(green collect) - Collect a single diagnostic immediately."
    echo
    white "Variables:"
    echo "  $(green APIKEY)           - The encoded Elasticsearch API key for cluster authentication."
    echo "  $(green URL)              - The Elasticsearch endpoint URL, no trailing slashes."
    echo "  $(green WAIT_SECONDS)     - Seconds between each collection start."
    echo "  $(green COLLECTION_COUNT) - Number of collections to perform."
    echo
}

# Saves a manifest file with metadata about the diagnostic collection.
# It retrieves the Elasticsearch version from the `version.json` file.
function save_manifest() {
    local VERSION && VERSION=$(jq -r '.version.number' "${DIR}/version.json")
    echo '{
    "mode" : "minimum",
    "product" : "elasticsearch",
    "flags" : "None",
    "diagnostic" : null,
    "type" : "elasticsearch_diagnostic",
    "runner" : "min-diag",
    "version" : "'"$VERSION"'",
    "timestamp" : "'"$(date -u +"%Y-%m-%dT%H:%M:%SZ")"'"
}' > "${DIR}/diagnostic_manifest.json"
}

# Check if curl command is available
if ! command -v curl > /dev/null; then
    log_error "$(red missing) required command $(white curl)."
    exit 1
else
    log_debug "$(green found) command $(white curl)"
fi

function get_api() {
    local API="$1"
    local OUTPUT="$2"

    log_info "$(green saving) $(cyan "${API}") to $(gray "${DIR}/${OUTPUT}")"
    curl --silent \
        --header "Authorization: ApiKey ${APIKEY}" \
        --header "X-Management-Request: true" \
        --output "$DIR/$OUTPUT" \
        "$URL/$API"
}

# Collects API calls from the Elasticsearch cluster and saves them to files. It
# creates a subdirectory with the current date and time, finally calling save_manifest.
function collect_diag() {
    local DATE && DATE=$(date +"%Y%m%d-%H%M%S")
    declare DIR="api-diagnostics-$DATE"
    log_info "$(green created) directory $(gray "$DIR")"

    mkdir -p "$DIR/commercial"
    get_api "/" version.json
    get_api "_alias" alias.json
    get_api "_cluster/settings?include_defaults&flat_settings" cluster_settings_defaults.json
    get_api "_data_stream?expand_wildcards=all" commercial/data_stream.json
    get_api "*/_ilm/explain?expand_wildcards=all" commercial/ilm_explain.json
    get_api "_ilm/policy" commercial/ilm_policies.json
    get_api "_settings?expand_wildcards=all" settings.json
    get_api "_stats?level=shards&expand_wildcards=all&ignore_unavailable=true" indices_stats.json
    get_api "_license" licenses.json
    get_api "_nodes" nodes.json
    get_api "_nodes/stats" nodes_stats.json
    get_api "_cluster/pending_tasks" cluster_pending_tasks.json
    get_api "_searchable_snapshots/cache/stats" commercial/searchable_snapshots_cache_stats.json
    get_api "_slm/policy" commercial/slm_policies.json
    get_api "_tasks?detailed=true" tasks.json
    save_manifest
}

# Watches a cluster and periodically collects diagnostics. It runs collect_diag
# COLLECTION_COUNT times, waiting WAIT_SECONDS seconds between each collection. This
# outputs the new directory name at the start of each diagnostic collection.
function watch() {
    log_info "$(green collecting) $(cyan "$COLLECTION_COUNT") diagnostics, $(cyan "$WAIT_SECONDS") seconds apart, from $(blue $URL)"
    for i in $(seq 1 ${COLLECTION_COUNT}); do
        log_info "$(green collecting) diagnostic $(cyan "$i") of $(cyan "$COLLECTION_COUNT")"
        # Running collections in the background allows sleep to start counting immediately
        collect_diag &
        # Skip the last sleep
        if [[ "${i}" -lt "${COLLECTION_COUNT}" ]]; then
            sleep ${WAIT_SECONDS}
        fi
    done
    # Wait so the last background diagnostic doesn't exit before collections finish
    wait $!
}

# Collects a single diagnostic from the cluster.
function collect() {
    log_info "$(green collecting) diagnostic from $(blue "$URL")"
    collect_diag
}

# ---------- Execution ----------

# Print help if requested and exit succesfully
if [[ "$1" == "help" || "$1" == "--help" || "$1" == "-h" ]]; then
    help
    exit 0
fi

# Verify required variables were set
if [[ -z "$APIKEY" || -z "$URL" ]]; then
    log_error "$(red missing) $(magenta URL) and $(magenta APIKEY) in the script."
    help
    exit 1
fi

# Run the appropriate function based on the command-line argument
if [[ "$1" == "watch" ]]; then
    watch
elif [[ "$1" == "collect" ]]; then
    collect
else
    log_error "$(red missing) command"
    help
    exit 1
fi
