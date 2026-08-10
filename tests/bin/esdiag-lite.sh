#!/usr/bin/env bash

set -eu

repo_root=$(cd "$(dirname "$0")/../.." && pwd)
script="$repo_root/bin/esdiag-lite.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

assert_equal() {
  [[ $1 == "$2" ]] || fail "expected '$1', got '$2'"
}

assert_contains() {
  case $1 in
    *"$2"*) ;;
    *) fail "expected '$1' to contain '$2'" ;;
  esac
}

assert_not_contains() {
  case $1 in
    *"$2"*) fail "did not expect '$1' to contain '$2'" ;;
    *) ;;
  esac
}

source "$script"

test_version_predicates() {
  ES_MAJOR=7
  ES_MINOR=10
  ES_PATCH=0
  version_at_least 7 7 0 || fail '7.10.0 should be at least 7.7.0'
  version_greater_than 7 7 0 || fail '7.10.0 should be greater than 7.7.0'
  ! version_less_than 7 7 0 || fail '7.10.0 should not be less than 7.7.0'
  version_at_most 7 10 0 || fail '7.10.0 should be at most 7.10.0'

  DIR="$tmp/version"
  mkdir -p "$DIR"
  printf '%s\n' '{"version":{"number":"9.0.0-SNAPSHOT"}}' >"$DIR/version.json"
  parse_cluster_version || fail 'valid prerelease version should parse'
  assert_equal '9.0.0-SNAPSHOT' "$CLUSTER_VERSION"
  assert_equal 9 "$ES_MAJOR"
  printf '%s\n' '{"version":{"number":"bad"}}' >"$DIR/version.json"
  ! parse_cluster_version || fail 'malformed version should fail'
}

test_sha256_provider() {
  select_sha256_provider || fail 'a supported SHA-256 provider should be available'
  digest=$(file_digest "$script") || fail 'SHA-256 digest should be calculated'
  [[ $digest =~ ^[0-9A-Fa-f]{64}$ ]] || fail 'SHA-256 digest should be hexadecimal'
}

test_generated_functions() {
  requests=
  skipped=
  get_api() { requests="$requests|$1:$2"; }
  skip_api() { skipped="$skipped|$1"; }
  CLUSTER_VERSION=7.10.0
  ES_MAJOR=7
  ES_MINOR=10
  ES_PATCH=0
  get_api_data_stream
  assert_contains "$requests" '/_data_stream:commercial/data_stream.json'
  get_api_searchable_snapshots_cache_stats
  assert_contains "$skipped" searchable_snapshots_cache_stats
  get_api_ilm_explain
  assert_contains "$requests" '/*/_ilm/explain?human&expand_wildcards=all:commercial/ilm_explain.json'
  get_api_settings
  assert_contains "$requests" '/_settings?human&expand_wildcards=all:settings.json'
}

test_generated_collection_failures() {
  requests=
  get_api() {
    requests="$requests|$1:$2"
    [[ $1 != '/_alias?human' ]]
  }
  skip_api() { :; }
  CLUSTER_VERSION=7.10.0
  ES_MAJOR=7
  ES_MINOR=10
  ES_PATCH=0
  ! collect_lite_apis || fail 'generated collection should report failed API requests'
  assert_contains "$requests" '/_tasks?human&detailed=true:tasks.json'
}

make_mock_path() {
  mock_bin="$tmp/mock-bin"
  mkdir -p "$mock_bin"
  cp "$repo_root/tests/bin/fixtures/esdiag-lite-curl" "$mock_bin/curl"
  cp "$repo_root/tests/bin/fixtures/esdiag-lite-zip" "$mock_bin/zip"
  chmod +x "$mock_bin/curl" "$mock_bin/zip"
}

run_collection() {
  run_dir=$1
  shift
  mkdir -p "$run_dir"
  (
    cd "$run_dir"
    PATH="$mock_bin:$PATH" \
      MOCK_CURL_LOG="$run_dir/curl.log" \
      MOCK_ZIP_LOG="$run_dir/zip.log" \
      ELASTIC_ES_URL='https://cluster.example:9200' \
      "$@"
  )
}

collection_dir() {
  for path in "$1"/api-diagnostics-*; do
    if [[ -d $path ]]; then
      printf '%s\n' "$path"
      return 0
    fi
  done
  return 1
}

archive_path() {
  for path in "$1"/api-diagnostics-*.zip; do
    if [[ -f $path ]]; then
      printf '%s\n' "$path"
      return 0
    fi
  done
  return 1
}

test_collection_authentication_and_none_output() {
  make_mock_path
  run_dir="$tmp/api-key"
  run_collection "$run_dir" env ELASTIC_ES_API_KEY='api-key' "$script" collect --archive=none
  directory=$(collection_dir "$run_dir") || fail 'none output should retain a directory'
  log=$(<"$run_dir/curl.log")
  assert_contains "$log" 'Authorization: ApiKey api-key'
  assert_not_contains "$log" --user
  assert_contains "$log" '/_settings?human&expand_wildcards=all'
  assert_not_contains "$log" searchable_snapshots_cache_stats
  assert_contains "$(<"$directory/diagnostic_manifest.json")" '"runner" : "esdiag-lite"'

  run_dir="$tmp/basic"
  run_collection "$run_dir" env ELASTIC_ES_USERNAME=user ELASTIC_ES_PASSWORD=pass "$script" collect --archive=none
  assert_contains "$(<"$run_dir/curl.log")" --user
  assert_contains "$(<"$run_dir/curl.log")" user:pass

  run_dir="$tmp/precedence"
  run_collection "$run_dir" env ELASTIC_ES_API_KEY=key ELASTIC_ES_USERNAME=user ELASTIC_ES_PASSWORD=pass "$script" collect --archive=none
  assert_contains "$(<"$run_dir/curl.log")" 'Authorization: ApiKey key'
  assert_not_contains "$(<"$run_dir/curl.log")" user:pass

  run_dir="$tmp/no-implicit-upload"
  run_collection "$run_dir" env ELASTIC_ES_API_KEY=key UPLOAD_ID=environment-id "$script" collect
  assert_not_contains "$(<"$run_dir/curl.log")" '/api/uploads/'
}

test_archives_and_validation() {
  make_mock_path
  run_dir="$tmp/zip"
  run_collection "$run_dir" env ELASTIC_ES_API_KEY=key "$script" collect
  archive_path "$run_dir" >/dev/null || fail 'default ZIP output should exist'
  if collection_dir "$run_dir" >/dev/null; then
    fail 'ZIP output should remove source directory'
  fi

  run_dir="$tmp/archive-failure"
  if run_collection "$run_dir" env MOCK_ZIP_FAIL=true ELASTIC_ES_API_KEY=key "$script" collect; then
    fail 'archive creation failure should fail collection'
  fi
  collection_dir "$run_dir" >/dev/null || fail 'failed archive should preserve directory'

  if run_collection "$tmp/unknown" env ELASTIC_ES_API_KEY=key "$script" collect --archive=tar; then
    fail 'unknown archive format should fail'
  fi

  curl_only="$tmp/curl-only"
  mkdir -p "$curl_only"
  cp "$repo_root/tests/bin/fixtures/esdiag-lite-curl" "$curl_only/curl"
  chmod +x "$curl_only/curl"
  if output=$(PATH="$curl_only" ELASTIC_ES_URL=url ELASTIC_ES_API_KEY=key /bin/bash "$script" collect 2>&1); then
    fail 'missing ZIP should fail'
  fi
  assert_equal 'No zip executable found, run with --archive=none to skip archive creation' "$output"

  if output=$(ELASTIC_ES_URL=url ELASTIC_ES_USERNAME=user bash "$script" collect --archive=none 2>&1); then
    fail 'incomplete authentication should fail'
  fi
  assert_not_contains "$output" user
}

test_uploads() {
  make_mock_path
  run_dir="$tmp/upload-after-collect"
  run_collection "$run_dir" env ELASTIC_ES_API_KEY=key "$script" collect --upload=immediate-id
  archive=$(archive_path "$run_dir") || fail 'immediate upload should retain archive'
  log=$(<"$run_dir/curl.log")
  assert_contains "$log" 'https://upload.elastic.co/api/uploads/immediate-id'
  assert_contains "$log" --head
  assert_contains "$log" --request
  assert_contains "$log" _finalize

  run_dir="$tmp/upload-existing"
  mkdir -p "$run_dir"
  printf 'existing archive' >"$run_dir/existing.zip"
  (
    cd "$run_dir"
    PATH="$mock_bin:$PATH" MOCK_CURL_LOG="$run_dir/curl.log" UPLOAD_ID=environment-id "$script" upload existing.zip
  ) || fail 'upload should use UPLOAD_ID when no argument is provided'
  assert_contains "$(<"$run_dir/curl.log")" 'https://upload.elastic.co/api/uploads/environment-id'

  run_dir="$tmp/upload-argument"
  mkdir -p "$run_dir"
  printf 'existing archive' >"$run_dir/existing.zip"
  (
    cd "$run_dir"
    PATH="$mock_bin:$PATH" MOCK_CURL_LOG="$run_dir/curl.log" UPLOAD_ID=environment-id "$script" upload existing.zip argument-id
  ) || fail 'upload should accept an explicit id'
  assert_contains "$(<"$run_dir/curl.log")" 'https://upload.elastic.co/api/uploads/argument-id'
  assert_not_contains "$(<"$run_dir/curl.log")" environment-id

  run_dir="$tmp/upload-resume"
  mkdir -p "$run_dir"
  printf 'existing archive' >"$run_dir/existing.zip"
  (
    cd "$run_dir"
    PATH="$mock_bin:$PATH" MOCK_CURL_LOG="$run_dir/curl.log" MOCK_UPLOAD_PART_EXISTS=true UPLOAD_ID=environment-id "$script" upload existing.zip
  ) || fail 'upload should skip an existing part'
  assert_not_contains "$(<"$run_dir/curl.log")" PUT

  if PATH="$mock_bin:$PATH" MOCK_CURL_LOG="$tmp/missing-upload-id.log" "$script" upload "$run_dir/existing.zip" >/dev/null 2>&1; then
    fail 'upload without an id should fail'
  fi
  if run_collection "$tmp/upload-none" env ELASTIC_ES_API_KEY=key "$script" collect --archive=none --upload=id; then
    fail 'immediate upload with directory output should fail'
  fi
}

test_version_predicates
test_sha256_provider
test_generated_functions
test_generated_collection_failures
test_collection_authentication_and_none_output
test_archives_and_validation
test_uploads
printf 'esdiag-lite shell tests passed\n'
