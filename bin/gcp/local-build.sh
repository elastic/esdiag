#!/bin/bash
set -e

# Builds the Docker image for the Elastic Stack Diagnostic Tool, tagging with current architecture and version
# Requires Docker registry pre-authentication

if [[ ! -f Cargo.toml ]]; then
    echo "Cargo.toml not found, run from repository root"
    exit 1
fi

declare arch=$(uname -m)
declare version=$(grep '^version =' Cargo.toml | sed 's/version = "\(.*\)"/\1/')

docker build ~/Development/esdiag \
    --tag "us-west2-docker.pkg.dev/elastic-customer-eng/esdiag/esdiag:latest-${arch}" \
    --tag "us-west2-docker.pkg.dev/elastic-customer-eng/esdiag/esdiag:${version}-${arch}" \
    --push
