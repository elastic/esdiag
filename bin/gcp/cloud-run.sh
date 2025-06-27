#!/bin/bash
set -e

# Executes a Google Cloud Run job
# Requires pre-authentication with gcloud CLI

if [ "$#" -ne 3 ]; then
    echo "Usage: ${0} <ENV> <TOKEN> <URL>"
    return 1
fi

local job="${1}"
local token="${2}"
local url="${3}"
local token_url="${url/https:\/\//https:\/\/token:${token}@}"

echo "Executing esdiag cloud run on: ${token_url}"
gcloud run jobs execute "${job}" --region=us-west2 --args="process,${token_url}"
