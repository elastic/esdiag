#!/usr/bin/env bash

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
local_script="${root}/bin/esdiag-local"
state_dir="${ESDIAG_LOCAL_TEST_STATE_DIR:-${1:-${HOME}/.esdiag/local}}"
runtime="${ESDIAG_CONTAINER_RUNTIME:-${2:-docker}}"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
for command in curl jq "$runtime"; do
    command -v "$command" >/dev/null 2>&1 || fail "Missing required command: ${command}"
done
[[ -f "${state_dir}/.env" && -f "${state_dir}/compose.yml" ]] || fail "No local stack state at ${state_dir}"

env_value() {
    sed -n "s/^$1=//p" "${state_dir}/.env" | tail -n 1
}

api_key=$($local_script secrets apikey --state-dir "$state_dir")
es_port=$(env_value ESDIAG_ELASTICSEARCH_PORT)
kibana_port=$(env_value ESDIAG_KIBANA_PORT)
web_port=$(env_value ESDIAG_PORT)
project_id=$(printf '%s' "$state_dir" | cksum | cut -d' ' -f1)
project_name="esdiag-local-${project_id}"

payload=$(jq -cn --arg apikey "$api_key" '{
    metadata: {
        account: "esdiag-local-self-test",
        case_number: null,
        opportunity: null,
        user: "local-self-test"
    },
    apikey: $apikey,
    url: "http://elasticsearch:9200"
}')

response=$(curl -fsS -X POST "http://127.0.0.1:${web_port}/api/api_key?wait_for_completion" \
    -H 'Content-Type: application/json' \
    --data-binary "$payload")
diagnostic_id=$(jq -er '[.[] | select(.status == "success" and .source == "parent")][0].diagnostic_id' <<<"$response")
[[ -n "$diagnostic_id" ]] || fail "Self-diagnostic response did not contain a diagnostic identifier"
kibana_link=$(jq -er '[.[] | select(.status == "success" and .source == "parent")][0].kibana_link' <<<"$response")
[[ "$kibana_link" == "http://localhost:${kibana_port}/"* ]] \
    || fail "Kibana link is not browser-reachable: ${kibana_link}"

auth_header="Authorization: ApiKey ${api_key}"
curl -fsS -X POST "http://127.0.0.1:${es_port}/metrics-diagnostic-esdiag/_refresh" -H "$auth_header" >/dev/null
hits=$(curl -fsS "http://127.0.0.1:${es_port}/metrics-diagnostic-esdiag/_search" \
    -H "$auth_header" \
    -H 'Content-Type: application/json' \
    --data-binary "$(jq -cn --arg id "$diagnostic_id" '{size: 0, query: {term: {"diagnostic.id": $id}}}')")
jq -e '.hits.total.value > 0' <<<"$hits" >/dev/null || fail "Self-diagnostic report was not indexed"

field_caps=$(curl -fsS "http://127.0.0.1:${es_port}/metrics-*-esdiag/_field_caps?fields=*" -H "$auth_header")
jq -e '.fields | length > 0 and has("diagnostic.id")' <<<"$field_caps" >/dev/null \
    || fail "Expected lazily materialized diagnostic mapping fields were not present"

if "$runtime" compose --project-name "$project_name" --project-directory "$state_dir" \
    --env-file "${state_dir}/.env" --file "${state_dir}/compose.yml" logs --no-color esdiag \
    | grep -Eq '(^|\| )[[:space:]]*\{.*"data_stream":\{"dataset":'; then
    fail "Processed document JSON was written to ESDiag container stdout"
fi

printf 'Self-diagnostic %s indexed successfully with environment-backed output\n' "$diagnostic_id"
