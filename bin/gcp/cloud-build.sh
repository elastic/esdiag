#!/bin/bash
set -e

# Submits a container build to Google Cloud Build
# Requires pre-authentication with gcloud CLI

if [[ ! -f Cargo.toml ]] && [[ ! -f cloudbuild.yml ]]; then
    echo "Cargo.toml not found, run from repository root"
    exit 1
fi

# Extract version from Cargo.toml
declare version=$(grep '^version =' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
echo "Submitting build for version: ${version}"
gcloud builds submit --substitutions=_VERSION="${version}" .
