ESDiag Additional Executables
=============================

1. [esdiag-docker.sh](#esdiag-dockersh)
2. [min-diag.sh](#min-diagsh)
3. [stack-local-setup.sh](#stack-local-setupsetupsh)

esdiag-docker.sh
-----------------

A wrapper script that allows running ESDiag inside a Docker container. Avoids installing any local Rust toolchain dependencies.

### Usage

Start by building the container image:

```sh
docker build --tag esdiag:latest .
```

When running from a container, the output is controlled through environment variables. The default url is effectively `http://localhost:9200` with no authentication (from inside the container it is `http://host.docker.internal:9200`). This can be overridden by setting the `ESDIAG_OUTPUT_*` environment variables, which are passed through to the container.

```sh
export ESDIAG_OUTPUT_URL="https://my-deployment.cloud.elastic.co"
export ESDIAG_OUTPUT_APIKEY="<apikey>"
export ESDIAG_OUTPUT_USERNAME="<username>"
export ESDIAG_OUTPUT_PASSWORD="<password>"
```

You do not need the username or password with an API key.

Then you can use the script to run ESDiag inside the container:

```sh
./esdiag-docker.sh <command> [arguments]
```

For example to run the `process` command:

```sh
./esdiag-docker.sh process /full/path/to/file.json
```

### Limitations

* Be sure to include the fully-qualified path to an input file or directory. This must be mounted as a Docker volume, and relative paths will not work.
* No `host` command support, it requires persistent storage and the container is run with `--rm` and removing it after every run.
* No `collect` command support, it requires input configurations which are not configurable through environment variables.
* No support for output to a local file or directory.
A simple wrapper script to run ESDiag from inside a Docker container. Avoids installing any local Rust dependencies.

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

stack-local-setup.sh
---------------------

Setup a complete, local, Docker container-powered Elastic Stack for ESDiag.

1. Builds an ESDiag container image for use with `esdiag-docker.sh`
2. Starts a security-disabled Kibana and Elasticsearch via `docker compose`
3. Sets up Elasticsearch templates via `esdiag setup`
4. Imports Kibana saved objects from an `.ndjson` file
5. Opens Kibana in your web browser

### Usage:

```sh
./bin/stack-local-setup.sh [path/to/dashboards.ndjson]
```

If no dashboard file is provided, the script will attempt to use the newest dashboard file in `assets/kibana/esdiag-dashboards*.ndjson`. Dashboard files can be downloaded from the [esdiag-dashboards](https://github.com/elastic/esdiag-dashboards/releases/latest) GitHub repository.

### Dependencies:

This script has the following dependencies:

1. Docker with `docker compose` support
2. `jq` - for json parsing
3. `curl` - for http requests
4. `grep` - for pattern matching
5. `sed` - for text manipulation
