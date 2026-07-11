#!/usr/bin/env bash

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
script="${root}/bin/esdiag-local"
real_podman=$(command -v podman || true)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
assert_contains() { grep -Fq -- "$2" "$1" || fail "$1 does not contain: $2"; }
assert_not_contains() { ! grep -Fq -- "$2" "$1" || fail "$1 contains secret/unexpected text: $2"; }
assert_mode() {
    local mode
    if [[ "$(uname -s)" == Darwin ]]; then mode=$(stat -f '%Lp' "$1"); else mode=$(stat -c '%a' "$1"); fi
    [[ "$mode" == "$2" ]] || fail "$1 does not have mode $2"
}
digest() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
run_local() {
    PATH="$fake_bin:$PATH" ESDIAG_TEST_MEMORY_MB=8192 ESDIAG_TEST_DISK_MB=8192 \
        "$script" "$@"
}

fake_bin="$tmp/bin"
mkdir -p "$fake_bin" "$tmp/work" "$tmp/volumes"
cat >"$fake_bin/podman" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"${FAKE_LOG}"
if [[ "$1 $2" == "volume inspect" ]]; then [[ -f "${FAKE_VOLUMES}/$3" ]]; exit; fi
if [[ "$1" == compose ]]; then
  project=""; action=""
  while [[ $# -gt 0 ]]; do
    case "$1" in --project-name) project=$2; shift 2 ;; up|down|run|ps|logs|version) action=$1; shift; break ;; *) shift ;; esac
  done
  if [[ "$action" == up ]]; then touch "${FAKE_VOLUMES}/${project}_elasticsearch-data" "${FAKE_VOLUMES}/${project}_kibana-data"; fi
  if [[ "$action" == down && "$*" == *--volumes* ]]; then rm -f "${FAKE_VOLUMES}/${project}_elasticsearch-data" "${FAKE_VOLUMES}/${project}_kibana-data"; fi
fi
exit 0
EOF
cat >"$fake_bin/curl" <<'EOF'
#!/usr/bin/env bash
output=""; url=""
while [[ $# -gt 0 ]]; do
  case "$1" in -o) output=$2; shift 2 ;; http*) url=$1; shift ;; *) shift ;; esac
done
if [[ -n "$output" && "$url" == *'/esdiag-local.sha256' ]]; then cp "$FAKE_CHECKSUM" "$output"; exit; fi
if [[ -n "$output" && "$url" == *'/esdiag-local' ]]; then
  [[ "${FAKE_CURL_FAIL:-false}" == true ]] && exit 22
  cp "$FAKE_CANDIDATE" "$output"; exit
fi
if [[ "$url" == *'_security/api_key' ]]; then printf '%s' '{"encoded":"fixture-api-key"}'; else printf '%s' '{}'; fi
EOF
cat >"$fake_bin/openssl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG'
EOF
cat >"$fake_bin/lsof" <<'EOF'
#!/usr/bin/env bash
[[ "$*" == *":${FAKE_BUSY_PORT:-none}"* ]]
EOF
for utility in pbcopy wl-copy xclip xsel clip.exe open xdg-open cmd.exe; do
    cat >"$fake_bin/$utility" <<'EOF'
#!/usr/bin/env bash
cat >>"${FAKE_CLIPBOARD:-/dev/null}" 2>/dev/null || true
printf '%s %s\n' "${0##*/}" "$*" >>"${FAKE_UI_LOG:-/dev/null}"
EOF
done
chmod +x "$fake_bin/"*

export FAKE_LOG="$tmp/runtime.log" FAKE_VOLUMES="$tmp/volumes" FAKE_BUSY_PORT=none
export FAKE_CLIPBOARD="$tmp/clipboard" FAKE_UI_LOG="$tmp/ui.log"

# Shell, help, platform adapters, and repository-independent execution.
bash -n "$script"
[[ "$($script version)" == "0.16.0" ]] || fail version
PATH="$fake_bin:$PATH" ESDIAG_TEST_OS=Darwin "$script" help >"$tmp/help-macos"
PATH="$fake_bin:$PATH" ESDIAG_TEST_OS=Linux "$script" help >"$tmp/help-linux"
PATH="$fake_bin:$PATH" ESDIAG_TEST_OS=Linux ESDIAG_TEST_WSL=true "$script" help >"$tmp/help-wsl"
assert_contains "$tmp/help-macos" 'secrets password | pbcopy'
assert_contains "$tmp/help-linux" 'secrets password | wl-copy'
assert_contains "$tmp/help-wsl" 'secrets password | clip.exe'
assert_not_contains "$tmp/help-linux" '--insecure'
if run_local up --runtime podman --state-dir "$tmp/rejected-insecure" --pull never --insecure 2>"$tmp/insecure-error"; then fail '--insecure was accepted'; fi
assert_contains "$tmp/insecure-error" 'security is required'
if command -v shellcheck >/dev/null 2>&1; then shellcheck "$script"; fi

(cd "$tmp/work" && run_local up --runtime podman --state-dir "$tmp/secure" --pull never --open-browser=false)
[[ -f "$tmp/secure/.env" && -f "$tmp/secure/compose.yml" ]] || fail 'state was not generated'
assert_mode "$tmp/secure" 700; assert_mode "$tmp/secure/.env" 600
assert_contains "$tmp/secure/compose.yml" 'Generated secure configuration'
assert_contains "$tmp/secure/compose.yml" "127.0.0.1:\${ESDIAG_PORT}:2501"
assert_contains "$tmp/secure/compose.yml" "ESDIAG_OUTPUT_APIKEY: \${ESDIAG_OUTPUT_APIKEY}"
[[ "$(grep -c "image: \${ESDIAG_IMAGE}" "$tmp/secure/compose.yml")" == 2 ]] || fail 'setup/service image mismatch'

# Idempotence, status/auth/log secrecy, raw secrets, setup, down, and reset.
password=$(sed -n 's/^ELASTIC_PASSWORD=//p' "$tmp/secure/.env")
run_local up --runtime podman --state-dir "$tmp/secure" --pull never --open-browser=false
[[ "$password" == "$(sed -n 's/^ELASTIC_PASSWORD=//p' "$tmp/secure/.env")" ]] || fail 'password rotated'
run_local setup --runtime podman --state-dir "$tmp/secure" >"$tmp/setup.out" 2>"$tmp/setup.err"
run_local status --runtime podman --state-dir "$tmp/secure" >"$tmp/status" 2>&1
run_local auth --state-dir "$tmp/secure" >"$tmp/auth" 2>&1
run_local logs --runtime podman --state-dir "$tmp/secure" >"$tmp/logs" 2>&1
for output in "$tmp/status" "$tmp/auth" "$tmp/logs"; do assert_not_contains "$output" "$password"; assert_not_contains "$output" fixture-api-key; done
[[ "$(run_local secrets password --state-dir "$tmp/secure")" == "$password" ]] || fail password
[[ "$(run_local secrets apikey --state-dir "$tmp/secure")" == fixture-api-key ]] || fail apikey
run_local down --runtime podman --state-dir "$tmp/secure"
[[ -f "$tmp/secure/.env" ]] || fail 'down removed state'
if run_local reset --runtime podman --state-dir "$tmp/secure" </dev/null 2>/dev/null; then fail 'reset lacked confirmation'; fi
run_local reset --runtime podman --state-dir "$tmp/secure" --force
[[ ! -e "$tmp/secure" ]] || fail reset

# Image/registry/version precedence and configurable port validation.
run_local up --runtime podman --state-dir "$tmp/overrides" --pull never --open-browser=false \
    --esdiag-registry registry.example --elastic-registry elastic.example --esdiag-version 1.2.3 --elastic-version 9.9.9
assert_contains "$tmp/overrides/.env" 'ESDIAG_IMAGE=registry.example/esdiag/esdiag:1.2.3'
assert_contains "$tmp/overrides/.env" 'ELASTICSEARCH_IMAGE=elastic.example/elasticsearch/elasticsearch:9.9.9'
ESDIAG_IMAGE_TAG=env:image run_local up --runtime podman --state-dir "$tmp/env-image" --pull never --open-browser=false
assert_contains "$tmp/env-image/.env" 'ESDIAG_IMAGE=env:image'
run_local up --runtime podman --state-dir "$tmp/cli-image" --pull never --image cli:image --open-browser=false
assert_contains "$tmp/cli-image/.env" 'ESDIAG_IMAGE=cli:image'
sed -i.bak -e 's/^ESDIAG_ELASTICSEARCH_PORT=.*/ESDIAG_ELASTICSEARCH_PORT=19200/' -e 's/^ESDIAG_KIBANA_PORT=.*/ESDIAG_KIBANA_PORT=15601/' -e 's/^ESDIAG_PORT=.*/ESDIAG_PORT=12501/' "$tmp/env-image/.env"
run_local up --runtime podman --state-dir "$tmp/env-image" --pull never --open-browser=false
sed -i.bak 's/^ESDIAG_PORT=.*/ESDIAG_PORT=15601/' "$tmp/env-image/.env"
if run_local up --runtime podman --state-dir "$tmp/env-image" --pull never --open-browser=false 2>"$tmp/ports-error"; then fail 'duplicate ports accepted'; fi
assert_contains "$tmp/ports-error" 'ports must be unique'
FAKE_BUSY_PORT=2501; export FAKE_BUSY_PORT
if run_local up --runtime podman --state-dir "$tmp/busy" --pull never --open-browser=false 2>"$tmp/busy-error"; then fail 'busy port accepted'; fi
assert_contains "$tmp/busy-error" 'Port 2501 is already in use'
sed -i.bak -e 's/^ESDIAG_ELASTICSEARCH_PORT=.*/ESDIAG_ELASTICSEARCH_PORT=19200/' -e 's/^ESDIAG_KIBANA_PORT=.*/ESDIAG_KIBANA_PORT=15601/' -e 's/^ESDIAG_PORT=.*/ESDIAG_PORT=3333/' "$tmp/busy/.env"
FAKE_BUSY_PORT=3333; export FAKE_BUSY_PORT
if run_local up --runtime podman --state-dir "$tmp/busy" --pull never --open-browser=false 2>"$tmp/custom-busy-error"; then fail 'custom busy port accepted'; fi
assert_contains "$tmp/custom-busy-error" 'Port 3333 is already in use'
FAKE_BUSY_PORT=none; export FAKE_BUSY_PORT

# Safe parser and credential/volume mismatch failures.
printf '%s\n' "ESDIAG_PORT=\$(touch /tmp/esdiag-local-eval)" >>"$tmp/env-image/.env"
if run_local status --runtime podman --state-dir "$tmp/env-image" >/dev/null 2>&1; then fail 'unsafe env accepted'; fi
[[ ! -e /tmp/esdiag-local-eval ]] || fail 'environment was evaluated'
mkdir -p "$tmp/missing-env"; project_id=$(printf '%s' "$tmp/missing-env" | cksum | cut -d' ' -f1)
touch "$tmp/volumes/esdiag-local-${project_id}_elasticsearch-data"
if run_local up --runtime podman --state-dir "$tmp/missing-env" --pull never 2>"$tmp/missing-env-error"; then fail 'missing env accepted'; fi
assert_contains "$tmp/missing-env-error" '.env is missing'
cp -R "$tmp/overrides" "$tmp/missing-volume"
if run_local up --runtime podman --state-dir "$tmp/missing-volume" --pull never 2>"$tmp/missing-volume-error"; then fail 'missing volumes accepted'; fi
assert_contains "$tmp/missing-volume-error" 'deployment volumes are missing'

# Script-versus-stack upgrade behavior.
sed -i.bak 's/^STACK_ESDIAG_VERSION=.*/STACK_ESDIAG_VERSION=0.1.0/' "$tmp/overrides/.env"
if run_local up --runtime podman --state-dir "$tmp/overrides" --pull never 2>"$tmp/upgrade-error"; then fail 'implicit upgrade accepted'; fi
assert_contains "$tmp/upgrade-error" 'up --upgrade'
run_local up --runtime podman --state-dir "$tmp/overrides" --pull never --upgrade --open-browser=false

# Clipboard ordering and opt-out.
: >"$tmp/clipboard"; : >"$tmp/ui.log"
PATH="$fake_bin:$PATH" ESDIAG_TEST_OS=Darwin ESDIAG_TEST_MEMORY_MB=8192 ESDIAG_TEST_DISK_MB=8192 \
    "$script" up --runtime podman --state-dir "$tmp/clipboard-state" --pull never --open-browser=true
[[ -s "$tmp/clipboard" ]] || fail 'password was not copied'
: >"$tmp/clipboard"
PATH="$fake_bin:$PATH" ESDIAG_TEST_OS=Darwin ESDIAG_TEST_MEMORY_MB=8192 ESDIAG_TEST_DISK_MB=8192 \
    "$script" up --runtime podman --state-dir "$tmp/clipboard-state" --pull never --open-browser=true --copy-password=false
[[ ! -s "$tmp/clipboard" ]] || fail 'password copied despite opt-out'

# Self-update: check-only, PATH invocation with spaces, checksum failure, symlink refusal, and isolation.
repo_hash=$(digest "$script")
install_dir="$tmp/install with spaces"; mkdir -p "$install_dir"
cp "$script" "$install_dir/esdiag-local"; chmod 755 "$install_dir/esdiag-local"
sed 's/readonly ESDIAG_VERSION="[^"]*"/readonly ESDIAG_VERSION="99.0.0"/' "$script" >"$tmp/candidate"
chmod 755 "$tmp/candidate"; printf '%s  esdiag-local\n' "$(digest "$tmp/candidate")" >"$tmp/checksum"
export FAKE_CANDIDATE="$tmp/candidate" FAKE_CHECKSUM="$tmp/checksum"
before=$(digest "$install_dir/esdiag-local")
cp "$script" "$tmp/current-candidate"; chmod 755 "$tmp/current-candidate"
FAKE_CANDIDATE="$tmp/current-candidate" PATH="$fake_bin:$install_dir:$PATH" esdiag-local update --check >"$tmp/current.out" 2>"$tmp/current.err"
assert_contains "$tmp/current.err" 'is current'
PATH="$fake_bin:$install_dir:$PATH" esdiag-local update --check >"$tmp/update-check.out" 2>"$tmp/update-check.err"
[[ "$before" == "$(digest "$install_dir/esdiag-local")" ]] || fail 'check mutated script'
PATH="$fake_bin:$install_dir:$PATH" esdiag-local update
[[ "$("$install_dir/esdiag-local" version)" == 99.0.0 ]] || fail 'PATH update failed'
cp "$script" "$install_dir/esdiag-local"; printf '%s\n' bad >"$tmp/checksum"
if PATH="$fake_bin:$install_dir:$PATH" esdiag-local update >/dev/null 2>"$tmp/checksum-error"; then fail 'bad checksum accepted'; fi
[[ "$before" == "$(digest "$install_dir/esdiag-local")" ]] || fail 'verification failure replaced script'
ln -s "$install_dir/esdiag-local" "$tmp/esdiag-link"
if PATH="$fake_bin:$PATH" "$tmp/esdiag-link" update >/dev/null 2>"$tmp/symlink-error"; then fail 'symlink update accepted'; fi
assert_contains "$tmp/symlink-error" 'Download manually'
printf '%s\n' '#!/usr/bin/env bash' 'echo 99.0.0' >"$tmp/candidate"
printf '%s  esdiag-local\n' "$(digest "$tmp/candidate")" >"$tmp/checksum"
if PATH="$fake_bin:$install_dir:$PATH" esdiag-local update >/dev/null 2>"$tmp/identity-error"; then fail 'invalid script identity accepted'; fi
assert_contains "$tmp/identity-error" 'invalid update identity'
FAKE_CURL_FAIL=true; export FAKE_CURL_FAIL
if PATH="$fake_bin:$install_dir:$PATH" esdiag-local update >/dev/null 2>"$tmp/network-error"; then fail 'network failure accepted'; fi
assert_contains "$tmp/network-error" 'no files changed'
unset FAKE_CURL_FAIL
readonly_dir="$tmp/readonly"; mkdir "$readonly_dir"; cp "$script" "$readonly_dir/esdiag-local"; chmod 555 "$readonly_dir"
if PATH="$fake_bin:$PATH" "$readonly_dir/esdiag-local" update >/dev/null 2>"$tmp/readonly-error"; then fail 'read-only update accepted'; fi
assert_contains "$tmp/readonly-error" 'Download manually'
chmod 755 "$readonly_dir"
if bash -c 'source "$1" update' _ "$install_dir/esdiag-local" >/dev/null 2>"$tmp/source-error"; then fail 'sourced execution accepted'; fi
[[ "$repo_hash" == "$(digest "$script")" ]] || fail 'repository script changed'

if [[ "${ESDIAG_TEST_COMPOSE_VALIDATE:-false}" == true ]]; then
    [[ -n "$real_podman" ]] || fail 'real Podman is unavailable for Compose validation'
    "$real_podman" compose --project-name esdiag-local-test --env-file "$tmp/overrides/.env" --file "$tmp/overrides/compose.yml" config >/dev/null
fi

printf 'esdiag-local tests passed\n'
