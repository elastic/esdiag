ESDiag Additional Executables
=============================

1. [esdiag-control](#esdiag-control)
2. [min-diag.sh](#min-diagsh)

esdiag-control
--------------

Setup a complete, local, Docker container-powered Elastic Stack for ESDiag.

1. Builds an ESDiag container image for use with `esdiag-docker.sh`
2. Starts a security-disabled Kibana and Elasticsearch via `docker compose`
3. Sets up Elasticsearch templates via `esdiag setup`
4. Imports Kibana saved objects from an `.ndjson` file
5. Opens Kibana in your web browser

### Dependencies

This script will check for the following dependencies:

1. `docker` - for containers, must include `docker compose` support
2. `jq` - for json parsing
3. `curl` - for http requests
4. `grep` - for pattern matching
5. `sed` - for text manipulation

### Usage

Before running the script, you'll need to either:
1. Create a [GitHub personal access token](../docs/github_token.md) and add a line to a `.env` file in the repository root:
   ```sh
   export GITHUB_TOKEN="github_pat_123..."
   ```
2. Manually download the latest dashboard file from the [esdiag-dashboards](https://github.com/elastic/esdiag-dashboards/releases/latest) GitHub repository and place it in the `assets/kibana` directory.

```sh
./bin/stack-local-setup.sh
```

If you provide a GitHub token, the script will do a version check, and download the latest dashboard release if compatible. Without a GitHub token, the script will use the current repository clone and rely on the manual download.

min-diag.sh
------------

A script to `collect` the minimum Elasticsearch diagnostic bundles required to
import into ESDiag; with a `watch` function to periodically collect at intervals.

As a portable bash script, it can be run on any system with bash installed. Authentication is handled through the `APIKEY` and `URL`  variables inside the script.

The `collect` command pulls one minimal diagnostic bundle from the cluster:

```bash
./min-diag.sh collect
```

Outputs one directory named `api-diagnostic-<timestamp>` with the diagnostic files in it.

The `watch` command periodically collects diagnostic bundles from the cluster:

```bash
./min-diag.sh watch
```

This outputs many directories named `api-diagnostic-<timestamp>`. The total number of collections, and the intervals between collections, are the `WAIT_TIME` and `COLLECTION_COUNT` variables inside the script.

Processing all of the diagnostic directories output by the `watch` command can be done with a single shell loop:

```bash
for DIR in api-diagnostic-*; do esdiag process $DIR localhost; done
```

Where `localhost` is a saved known host in the `~/.esdiag/hosts.yml` configuration file.
