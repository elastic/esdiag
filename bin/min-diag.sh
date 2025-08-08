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
# Outputs one directory named `api-diagnostic-<timestamp>` with the diagnostic files in it.
#
# The `watch` command periodically collects diagnostic bundles from the cluster:
#
# ```bash
# ./min-diag.sh watch
# ```
#
# Outputs many directories named `api-diagnostic-<timestamp>`.
#
# Only modify the variables in the `Configuration` section for your environment.

# ---------- Configuration ----------

# Use the encoded Elasticsearch API key for cluster authentication
declare APIKEY=""
# Use the Elasticsearch endpoint URL, no trailing slashes
declare URL=""

# Seconds between each collection start, be sure to wait long enough not to overlap collections
declare WAIT_TIME=60
# Number of collections to perform, for example a 60 second wait * 60 collections = 30 minutes
declare COLLECTION_COUNT=30

# ---------- Funcitons ----------

# Prints script usage information
function help() {
    echo "Usage: $0 {watch|collect|help}"
    echo
    echo "Commands:"
    echo "  watch   - Collect diagnostics periodically based on the configured WAIT_TIME and COLLECTION_COUNT."
    echo "  collect - Collect a single diagnostic immediately."
    echo
    echo "Variables:"
    echo "  APIKEY           - The encoded Elasticsearch API key for cluster authentication."
    echo "  URL              - The Elasticsearch endpoint URL, no trailing slashes."
    echo "  WAIT_TIME        - Seconds between each collection start."
    echo "  COLLECTION_COUNT - Number of collections to perform."
    echo
}

# Saves a manifest file with metadata about the diagnostic collection.
# It retrieves the Elasticsearch version from the `version.json` file.
function save_manifest() {
    local VERSION=$(jq -r '.version.number' version.json)
    echo '{
    "mode" : "minimum",
    "product" : "elasticsearch",
    "flags" : "None",
    "diagnostic" : null,
    "type" : "elasticsearch_diagnostic",
    "runner" : "min-diag",
    "version" : "'$VERSION'",
    "timestamp" : "'$(date -u +"%Y-%m-%dT%H:%M:%SZ")'"
}' > diagnostic_manifest.json
}

# Collects API calls from the Elasticsearch cluster and saves them to files. It
# creates a subdirectory with the current date and time, finally calling save_manifest.
function collect_diag() {
    local DATE=$(date +"%Y%m%d-%H%M%S")
    local DIR="api-diagnostic-$DATE"
    echo "$DIR"

    mkdir -p $DIR/commercial
    cd $DIR
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_alias" -o alias.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_cluster/settings?include_defaults&flat_settings" -o cluster_settings_defaults.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_data_stream?expand_wildcards=all" -o commercial/data_stream.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/*/_ilm/explain?expand_wildcards=all" -o commercial/ilm_explain.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_ilm/policy" -o commercial/ilm_policies.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_settings?expand_wildcards=all" -o settings.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_stats?level=shards&expand_wildcards=all&ignore_unavailable=true" -o indices_stats.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_license" -o licenses.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_nodes" -o nodes.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_nodes/stats" -o nodes_stats.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_cluster/pending_tasks" -o cluster_pending_tasks.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_searchable_snapshots/cache/stats" -o commercial/searchable_snapshots_cache_stats.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_slm/policy" -o commercial/slm_policies.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/_tasks?detailed=true" -o tasks.json
    curl --silent -H "Authorization: ApiKey $APIKEY" "$URL/" -o version.json
    save_manifest
    cd ..
}

# Watches a cluster and periodically collects diagnostics. It runs collect_diag
# COLLECTION_COUNT times, waiting WAIT_TIME seconds between each collection. This
# outputs the new directory name at the start of each diagnostic collection.
function watch() {
    echo "Collecting $COLLECTION_COUNT diagnostics, $WAIT_TIME seconds apart, from $URL"
    for i in $(seq 1 ${COLLECTION_COUNT}); do
        echo -n "Collecting diag ${i}: "
        # Running collections in the background allows sleep to start counting immediately
        collect_diag &
        # Skip the last sleep
        if [ $i -lt ${COLLECTION_COUNT} ]; then
            sleep ${WAIT_TIME}
        fi
    done
}

# Collects a single diagnostic from the cluster.
function collect() {
    echo "Collecting diagnostic from $URL"
    echo -n "Saving diag ${i}: "
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
    echo "ERROR: Must set URL and APIKEY variables in the script."
    echo
    help
    exit 1
fi

# Run the appropriate function based on the command-line argument
if [[ "$1" == "watch" ]]; then
    watch
elif [[ "$1" == "collect" ]]; then
    collect
else
    echo "ERROR: No command given."
    echo
    help
    exit 1
fi
