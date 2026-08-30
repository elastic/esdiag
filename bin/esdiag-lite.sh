#!/usr/bin/env bash

# ESDiag Lite is a collection-only Elasticsearch diagnostic utility. It saves
# raw API responses for later processing with `esdiag process`. It can
# optionally send the generated ZIP archive to Elastic Upload Service; it
# does not process, analyze, transform, export, or visualize diagnostics.
#
# Runtime requirements: Bash 3.2+, curl, and standard POSIX utilities. ZIP
# output (the default) additionally requires zip. Generated API functions are
# maintained from assets/elasticsearch/sources.yml by esdiag-lite-generate.

WAIT_SECONDS=${WAIT_SECONDS:-60}
COLLECTION_COUNT=${COLLECTION_COUNT:-5}
LOG_LEVEL=${LOG_LEVEL:-info}

LOG_NAME=esdiag-lite
ARCHIVE_FORMAT=zip
COMMAND=
AUTH_MODE=
DIR=
UPLOAD_HOST=${UPLOAD_HOST:-https://upload.elastic.co}
UPLOAD_ID=${UPLOAD_ID:-}
UPLOAD_FILE=
UPLOAD_REQUESTED=false
SHA256_PROVIDER=
CLUSTER_VERSION=
ES_MAJOR=
ES_MINOR=
ES_PATCH=

COLORIZE=false
if [[ -t 1 ]]; then
  COLORIZE=true
fi

echo_color() {
  local color=$1
  shift
  if [[ $COLORIZE == true ]]; then
    printf '\033[%sm%s\033[39m' "$color" "$*"
  else
    printf '%s' "$*"
  fi
}

red() { echo_color 31 "$@"; }
green() { echo_color 32 "$@"; }
yellow() { echo_color 33 "$@"; }
blue() { echo_color 34 "$@"; }
cyan() { echo_color 36 "$@"; }
gray() { echo_color 90 "$@"; }
magenta() { echo_color 35 "$@"; }
white() { echo_color 97 "$@"; }

timestamp() {
  date -u +"%Y-%m-%d %H:%M:%S"
}

log_error() {
  printf '[%s %s %s] %s\n' "$(timestamp)" "$(red Error)" "$LOG_NAME" "$*" >&2
}

log_warn() {
  printf '[%s %s %s] %s\n' "$(timestamp)" "$(yellow Warn)" "$LOG_NAME" "$*"
}

log_info() {
  printf '[%s %s %s] %s\n' "$(timestamp)" "$(green Info)" "$LOG_NAME" "$*"
}

log_debug() {
  if [[ $LOG_LEVEL == debug ]]; then
    printf '[%s %s %s] %s\n' "$(timestamp)" "$(blue Debug)" "$LOG_NAME" "$*"
  fi
}

help() {
  white "Usage: $(green "$0") <COMMAND> [OPTIONS]"
  printf '\n'
  white 'Commands:'
  printf '\n'
  printf '  %s - Collect diagnostics periodically based on WAIT_SECONDS and COLLECTION_COUNT.\n' "$(green watch)"
  printf '  %s - Collect a single diagnostic immediately.\n' "$(green collect)"
  printf '  %s - Send an existing ZIP archive to Elastic Upload Service; uses UPLOAD_ID when id is omitted.\n' "$(green 'send <filename> [id]')"
  printf '\n'
  white 'Options:'
  printf '\n'
  printf '  %s - Output format; zip is the default and none preserves the directory.\n' "$(green --archive=zip\|none)"
  printf '  %s - Send the generated ZIP archive to an Elastic Upload Service ID.\n' "$(green --send=UPLOAD_ID)"
  printf '\n'
  white 'Environment:'
  printf '\n'
  printf '  %s - Elasticsearch endpoint URL.\n' "$(green ELASTIC_ES_URL)"
  printf '  %s - Encoded Elasticsearch API key; takes precedence over basic authentication.\n' "$(green ELASTIC_ES_API_KEY)"
  printf '  %s - Username for HTTP basic authentication.\n' "$(green ELASTIC_ES_USERNAME)"
  printf '  %s - Password for HTTP basic authentication.\n' "$(green ELASTIC_ES_PASSWORD)"
  printf '  %s - Elastic Upload Service base URL; defaults to https://upload.elastic.co.\n' "$(green UPLOAD_HOST)"
  printf '  %s - Elastic Upload Service ID used by send when [id] is omitted.\n' "$(green UPLOAD_ID)"
  printf '\n'
  white 'ESDiag Lite collects raw diagnostic API responses and can forward its ZIP output. Process its ZIP or directory output with esdiag process.'
  printf '\n'
}

parse_arguments() {
  case ${1:-} in
    help | --help | -h)
      COMMAND=help
      return 0
      ;;
    collect | watch)
      COMMAND=$1
      shift
      ;;
    send)
      COMMAND=send
      shift
      if [[ $# -lt 1 || $# -gt 2 ]]; then
        log_error 'send requires a filename and accepts an optional Elastic Upload Service ID'
        help
        return 1
      fi
      UPLOAD_FILE=$1
      UPLOAD_REQUESTED=true
      if [[ $# -eq 2 ]]; then
        UPLOAD_ID=$2
      fi
      return 0
      ;;
    *)
      log_error 'missing or unknown command'
      help
      return 1
      ;;
  esac

  while [[ $# -gt 0 ]]; do
    case $1 in
      --archive=zip)
        ARCHIVE_FORMAT=zip
        ;;
      --archive=none)
        ARCHIVE_FORMAT=none
        ;;
      --archive=*)
        log_error 'archive must be zip or none'
        help
        return 1
        ;;
      --send=*)
        UPLOAD_ID=${1#--send=}
        if [[ -z $UPLOAD_ID ]]; then
          log_error 'Elastic Upload Service ID must not be empty'
          return 1
        fi
        UPLOAD_REQUESTED=true
        ;;
      *)
        log_error "unknown argument: $1"
        help
        return 1
        ;;
    esac
    shift
  done
}

validate_configuration() {
  if [[ -z ${ELASTIC_ES_URL:-} ]]; then
    log_error 'ELASTIC_ES_URL must be set'
    return 1
  fi

  if [[ -n ${ELASTIC_ES_API_KEY:-} ]]; then
    AUTH_MODE=api_key
    return 0
  fi

  if [[ -n ${ELASTIC_ES_USERNAME:-} && -n ${ELASTIC_ES_PASSWORD:-} ]]; then
    AUTH_MODE=basic
    return 0
  fi

  log_error 'a complete ELASTIC_ES_API_KEY or ELASTIC_ES_USERNAME/ELASTIC_ES_PASSWORD pair is required'
  return 1
}

validate_dependencies() {
  if ! command -v curl >/dev/null 2>&1; then
    log_error 'missing required command curl'
    return 1
  fi

  if [[ $COMMAND != send && $ARCHIVE_FORMAT == zip ]] && ! command -v zip >/dev/null 2>&1; then
    printf '%s\n' 'No zip executable found, run with --archive=none to skip archive creation' >&2
    return 1
  fi

  if [[ $UPLOAD_REQUESTED == true ]]; then
    if ! select_sha256_provider; then
      log_error 'missing SHA-256 command; install shasum, sha256sum, or openssl'
      return 1
    fi
    if ! command -v split >/dev/null 2>&1; then
      log_error 'missing required command split for Elastic Upload Service sends'
      return 1
    fi
  fi
}

validate_upload_configuration() {
  if [[ -z $UPLOAD_ID ]]; then
    log_error 'Elastic Upload Service ID must be provided as [id] or UPLOAD_ID'
    return 1
  fi
  if [[ ! -f $UPLOAD_FILE ]]; then
    log_error "send file does not exist: $UPLOAD_FILE"
    return 1
  fi
}

normalize_upload_id() {
  local id=${1%/}
  printf '%s\n' "${id##*/}"
}

select_sha256_provider() {
  if command -v shasum >/dev/null 2>&1; then
    SHA256_PROVIDER=shasum
  elif command -v sha256sum >/dev/null 2>&1; then
    SHA256_PROVIDER=sha256sum
  elif command -v openssl >/dev/null 2>&1; then
    SHA256_PROVIDER=openssl
  else
    return 1
  fi
}

file_digest() {
  local output
  local digest

  case $SHA256_PROVIDER in
    shasum)
      output=$(shasum -a 256 "$1") || return 1
      digest=${output%% *}
      ;;
    sha256sum)
      output=$(sha256sum "$1") || return 1
      digest=${output%% *}
      ;;
    openssl)
      output=$(openssl dgst -sha256 "$1") || return 1
      digest=${output##* }
      ;;
    *)
      return 1
      ;;
  esac

  if [[ ! $digest =~ ^[0-9A-Fa-f]{64}$ ]]; then
    return 1
  fi
  printf '%s\n' "$digest"
}

create_upload_temp_dir() {
  local base_dir=${TMPDIR:-/tmp}
  local attempt=0
  local temp_dir

  while [[ $attempt -lt 100 ]]; do
    temp_dir="$base_dir/esdiag-lite-send-$$-$attempt"
    if (umask 077 && mkdir "$temp_dir") >/dev/null 2>&1; then
      printf '%s\n' "$temp_dir"
      return 0
    fi
    attempt=$((attempt + 1))
  done
  return 1
}

send_diagnostic() (
  local upload_id
  local upload_host
  local temp_dir
  local file_name
  local file_size
  local file_digest_value
  local part
  local part_digest
  local part_number=1
  local part_size=50000000

  if ! validate_upload_configuration; then
    return 1
  fi
  upload_id=$(normalize_upload_id "$UPLOAD_ID")
  upload_host=${UPLOAD_HOST%/}
  file_name=$(basename "$UPLOAD_FILE")
  file_size=$(wc -c <"$UPLOAD_FILE") || return 1
  file_size=${file_size//[[:space:]]/}
  file_digest_value=$(file_digest "$UPLOAD_FILE") || {
    log_error "failed to calculate SHA-256 for $UPLOAD_FILE"
    return 1
  }
  temp_dir=$(create_upload_temp_dir) || {
    log_error 'failed to create temporary send directory'
    return 1
  }
  trap 'rm -rf "$temp_dir"' EXIT HUP INT TERM

  if [[ $file_size -lt $part_size ]]; then
    cp "$UPLOAD_FILE" "$temp_dir/part-aa" || {
      log_error "failed to create Elastic Upload Service part"
      return 1
    }
  else
    split -b "$part_size" "$UPLOAD_FILE" "$temp_dir/part-" || {
      log_error "failed to split $UPLOAD_FILE for Elastic Upload Service"
      return 1
    }
  fi

  log_info "$(green sending) $(gray "$UPLOAD_FILE") to Elastic Upload Service at $(blue "$upload_host")"
  for part in "$temp_dir"/part-*; do
    part_digest=$(file_digest "$part") || {
      log_error "failed to calculate SHA-256 for $part"
      return 1
    }
    if curl --fail --silent --show-error --output /dev/null --head \
      "$upload_host/api/uploads/$upload_id/$file_digest_value/$part_digest"; then
      log_info "$(yellow skipping) existing Elastic Upload Service part $(cyan "$part_number")"
    elif curl --fail --silent --show-error --request PUT \
      "$upload_host/api/uploads/$upload_id?part_number=$part_number&part_digest=$part_digest&file_digest=$file_digest_value&filename=$file_name" \
      --data-binary "@$part"; then
      log_info "$(green sent) part $(cyan "$part_number")"
    else
      log_error "Elastic Upload Service rejected part $part_number"
      return 1
    fi
    part_number=$((part_number + 1))
  done

  if curl --fail --silent --show-error --request POST \
    "$upload_host/api/uploads/$upload_id/$file_digest_value/_finalize"; then
    log_info "$(green sent) $(gray "$UPLOAD_FILE") to Elastic Upload Service"
    return 0
  fi
  log_error "failed to finalize Elastic Upload Service send for $UPLOAD_FILE"
  return 1
)

# Compares the validated Elasticsearch version numerically. The generator uses
# these helpers to lower sources.yml predicates without relying on sort -V or
# string comparisons.
version_at_least() {
  if [[ $ES_MAJOR -gt $1 ]]; then
    return 0
  elif [[ $ES_MAJOR -lt $1 ]]; then
    return 1
  elif [[ $ES_MINOR -gt $2 ]]; then
    return 0
  elif [[ $ES_MINOR -lt $2 ]]; then
    return 1
  elif [[ $ES_PATCH -ge $3 ]]; then
    return 0
  fi
  return 1
}

version_greater_than() {
  if [[ $ES_MAJOR -gt $1 ]]; then
    return 0
  elif [[ $ES_MAJOR -lt $1 ]]; then
    return 1
  elif [[ $ES_MINOR -gt $2 ]]; then
    return 0
  elif [[ $ES_MINOR -lt $2 ]]; then
    return 1
  elif [[ $ES_PATCH -gt $3 ]]; then
    return 0
  fi
  return 1
}

version_at_most() {
  if [[ $ES_MAJOR -lt $1 ]]; then
    return 0
  elif [[ $ES_MAJOR -gt $1 ]]; then
    return 1
  elif [[ $ES_MINOR -lt $2 ]]; then
    return 0
  elif [[ $ES_MINOR -gt $2 ]]; then
    return 1
  elif [[ $ES_PATCH -le $3 ]]; then
    return 0
  fi
  return 1
}

version_less_than() {
  if [[ $ES_MAJOR -lt $1 ]]; then
    return 0
  elif [[ $ES_MAJOR -gt $1 ]]; then
    return 1
  elif [[ $ES_MINOR -lt $2 ]]; then
    return 0
  elif [[ $ES_MINOR -gt $2 ]]; then
    return 1
  elif [[ $ES_PATCH -lt $3 ]]; then
    return 0
  fi
  return 1
}

parse_cluster_version() {
  local extracted
  local numeric_version
  local remainder

  extracted=$(awk '
    {
      rest = $0
      while (match(rest, /"number"[[:space:]]*:[[:space:]]*"[^"]+"/)) {
        value = substr(rest, RSTART, RLENGTH)
        sub(/^[^\"]*"number"[[:space:]]*:[[:space:]]*"/, "", value)
        sub(/"$/, "", value)
        print value
        rest = substr(rest, RSTART + RLENGTH)
      }
    }
  ' "$DIR/version.json")

  if [[ $(printf '%s\n' "$extracted" | awk 'NF { count += 1 } END { print count + 0 }') -ne 1 ]]; then
    log_error 'could not extract exactly one version.number from version.json'
    return 1
  fi

  CLUSTER_VERSION=$extracted
  if [[ ! $CLUSTER_VERSION =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
    log_error 'version.number is not a valid Elasticsearch version'
    return 1
  fi

  numeric_version=${CLUSTER_VERSION%%[-+]*}
  ES_MAJOR=${numeric_version%%.*}
  remainder=${numeric_version#*.}
  ES_MINOR=${remainder%%.*}
  ES_PATCH=${remainder#*.}
}

save_manifest() {
  printf '{\n    "mode" : "minimum",\n    "product" : "elasticsearch",\n    "flags" : "None",\n    "diagnostic" : null,\n    "type" : "elasticsearch_diagnostic",\n    "runner" : "esdiag-lite",\n    "version" : "%s",\n    "timestamp" : "%s"\n}\n' \
    "$CLUSTER_VERSION" "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" >"$DIR/diagnostic_manifest.json"
}

get_api() {
  local api=$1
  local output=$2
  local output_path="$DIR/$output"
  local request_url="${ELASTIC_ES_URL%/}$api"

  mkdir -p "$(dirname "$output_path")" || return 1
  log_info "$(green saving) $(cyan "$api") to $(gray "$output_path")"

  if [[ $AUTH_MODE == api_key ]]; then
    curl --fail --silent --show-error \
      --header "Authorization: ApiKey ${ELASTIC_ES_API_KEY}" \
      --header 'X-Management-Request: true' \
      --output "$output_path" \
      "$request_url"
  else
    curl --fail --silent --show-error \
      --user "${ELASTIC_ES_USERNAME}:${ELASTIC_ES_PASSWORD}" \
      --header 'X-Management-Request: true' \
      --output "$output_path" \
      "$request_url"
  fi
}

skip_api() {
  log_info "$(yellow skipping) $(cyan "$1") because it is unsupported on Elasticsearch $CLUSTER_VERSION"
}

# BEGIN GENERATED LITE APIS
# This region is generated by `cargo run --bin esdiag-lite-generate`. Do not edit.

get_api_alias() {
  if version_at_least 0 9 0; then
    get_api "/_alias?human" "alias.json"
  else
    skip_api "alias"
  fi
}

get_api_cluster_pending_tasks() {
  if version_at_least 0 9 0; then
    get_api "/_cluster/pending_tasks?human" "cluster_pending_tasks.json"
  else
    skip_api "cluster_pending_tasks"
  fi
}

get_api_cluster_settings_defaults() {
  if version_at_least 6 4 0; then
    get_api "/_cluster/settings?include_defaults&flat_settings" "cluster_settings_defaults.json"
  else
    skip_api "cluster_settings_defaults"
  fi
}

get_api_data_stream() {
  if version_at_least 7 11 0; then
    get_api "/_data_stream?expand_wildcards=all" "commercial/data_stream.json"
  elif version_at_least 7 9 0 && version_less_than 7 11 0; then
    get_api "/_data_stream" "commercial/data_stream.json"
  else
    skip_api "data_stream"
  fi
}

get_api_ilm_explain() {
  if version_at_least 6 6 0 && version_less_than 7 7 0; then
    get_api "/*/_ilm/explain?human" "commercial/ilm_explain.json"
  elif version_at_least 7 7 0; then
    get_api "/*/_ilm/explain?human&expand_wildcards=all" "commercial/ilm_explain.json"
  else
    skip_api "ilm_explain"
  fi
}

get_api_ilm_policies() {
  if version_at_least 6 6 0; then
    get_api "/_ilm/policy?human" "commercial/ilm_policies.json"
  else
    skip_api "ilm_policies"
  fi
}

get_api_indices_settings() {
  if version_at_least 0 9 0 && version_less_than 7 7 0; then
    get_api "/_settings?human" "settings.json"
  elif version_at_least 7 7 0; then
    get_api "/_settings?human&expand_wildcards=all" "settings.json"
  else
    skip_api "indices_settings"
  fi
}

get_api_indices_stats() {
  if version_at_least 0 9 0 && version_less_than 7 7 0; then
    get_api "/_stats?level=shards&human" "indices_stats.json"
  elif version_at_least 7 7 0; then
    get_api "/_stats?level=shards&human&expand_wildcards=all" "indices_stats.json"
  else
    skip_api "indices_stats"
  fi
}

get_api_licenses() {
  if version_at_least 1 0 0 && version_less_than 2 0 0; then
    get_api "/_licenses" "licenses.json"
  elif version_at_least 2 0 0 && version_less_than 7 6 0; then
    get_api "/_license" "licenses.json"
  elif version_at_least 7 6 0 && version_less_than 8 0 0; then
    get_api "/_license?accept_enterprise=true" "licenses.json"
  elif version_at_least 8 0 0; then
    get_api "/_license" "licenses.json"
  else
    skip_api "licenses"
  fi
}

get_api_nodes() {
  if version_at_least 0 9 0; then
    get_api "/_nodes?human" "nodes.json"
  else
    skip_api "nodes"
  fi
}

get_api_nodes_stats() {
  if version_at_least 0 9 0; then
    get_api "/_nodes/stats?human" "nodes_stats.json"
  else
    skip_api "nodes_stats"
  fi
}

get_api_searchable_snapshots_cache_stats() {
  if version_at_least 7 13 0; then
    get_api "/_searchable_snapshots/cache/stats" "commercial/searchable_snapshots_cache_stats.json"
  else
    skip_api "searchable_snapshots_cache_stats"
  fi
}

get_api_slm_policies() {
  if version_at_least 7 4 0; then
    get_api "/_slm/policy?human" "commercial/slm_policies.json"
  else
    skip_api "slm_policies"
  fi
}

get_api_tasks() {
  if version_at_least 2 0 0; then
    get_api "/_tasks?human&detailed=true" "tasks.json"
  else
    skip_api "tasks"
  fi
}

get_api_version() {
  get_api "/" "version.json"
}

collect_lite_apis() {
  local status=0
  get_api_alias || status=1
  get_api_cluster_pending_tasks || status=1
  get_api_cluster_settings_defaults || status=1
  get_api_data_stream || status=1
  get_api_ilm_explain || status=1
  get_api_ilm_policies || status=1
  get_api_indices_settings || status=1
  get_api_indices_stats || status=1
  get_api_licenses || status=1
  get_api_nodes || status=1
  get_api_nodes_stats || status=1
  get_api_searchable_snapshots_cache_stats || status=1
  get_api_slm_policies || status=1
  get_api_tasks || status=1
  return "$status"
}
# END GENERATED LITE APIS

archive_diagnostic() {
  local archive_path="${DIR}.zip"

  if [[ $ARCHIVE_FORMAT == none ]]; then
    log_info "$(green completed) directory $(gray "$DIR")"
    return 0
  fi

  if (cd "$DIR" && zip -rq "../$archive_path" .); then
    rm -rf "$DIR"
    UPLOAD_FILE=$archive_path
    log_info "$(green completed) archive $(gray "$archive_path")"
    return 0
  fi

  log_error "failed to create archive $(gray "$archive_path"); preserving $(gray "$DIR")"
  return 1
}

collect_diag() {
  local date
  date=$(date +"%Y%m%d-%H%M%S")
  DIR="api-diagnostics-$date"

  if ! mkdir -p "$DIR/commercial"; then
    log_error "failed to create directory $DIR"
    return 1
  fi
  log_info "$(green created) directory $(gray "$DIR")"

  if ! get_api_version; then
    log_error 'failed to fetch Elasticsearch root response'
    return 1
  fi
  if ! parse_cluster_version; then
    return 1
  fi

  if ! collect_lite_apis; then
    log_error 'one or more Elasticsearch API requests failed'
    return 1
  fi
  save_manifest
  if ! archive_diagnostic; then
    return 1
  fi
  if [[ $UPLOAD_REQUESTED == true ]]; then
    send_diagnostic
  fi
}

watch() {
  local i
  local pid
  local status=0
  local pids=()

  log_info "$(green collecting) $(cyan "$COLLECTION_COUNT") diagnostics, $(cyan "$WAIT_SECONDS") seconds apart, from $(blue "$ELASTIC_ES_URL")"
  for ((i = 1; i <= COLLECTION_COUNT; i += 1)); do
    log_info "$(green collecting) diagnostic $(cyan "$i") of $(cyan "$COLLECTION_COUNT")"
    collect_diag &
    pids+=("$!")
    if [[ $i -lt $COLLECTION_COUNT ]]; then
      sleep "$WAIT_SECONDS"
    fi
  done

  for pid in "${pids[@]}"; do
    if ! wait "$pid"; then
      status=1
    fi
  done
  return "$status"
}

collect() {
  log_info "$(green collecting) diagnostic from $(blue "$ELASTIC_ES_URL")"
  collect_diag
}

main() {
  if ! parse_arguments "$@"; then
    return 1
  fi
  if [[ $COMMAND == help ]]; then
    help
    return 0
  fi
  if [[ $COMMAND == send ]]; then
    if ! validate_upload_configuration || ! validate_dependencies; then
      return 1
    fi
    send_diagnostic
    return $?
  fi
  if [[ $UPLOAD_REQUESTED == true && $ARCHIVE_FORMAT != zip ]]; then
    log_error 'Elastic Upload Service sends require --archive=zip'
    return 1
  fi
  if ! validate_configuration || ! validate_dependencies; then
    return 1
  fi

  if [[ $COMMAND == watch ]]; then
    watch
  else
    collect
  fi
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  main "$@"
fi
