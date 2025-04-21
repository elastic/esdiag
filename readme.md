Elastic Stack Diagnostics
==========================

The Elastic Stack Diagnostics (`esdiag`) tool simplifies processing and importing diagnostic bundles into Elasticsearch. It pre-processes, split and enriches the raw API outputs into Elasticsearch-friendly documents. This makes building diagnostic Kibana dashboards, ES|QL queries, and more, easy.

Running locally with Docker Desktop
------------------------------------

Use the `bin/esdiag-docker.sh` wrapper script to run the tool in a local Docker container. This will build using the official Rust Docker image, and does not require any local tooling beyond Docker desktop (or Podman if you are adventurous).

### Usage

Start by building the container image:

```sh
docker build --tag esdiag:latest .
```

When running from a container, the output is controlled through environment variables. The default url is effectively `http://localhost:9200` with no authentication. This can be overridden by setting the `ESDIAG_OUTPUT_*` environment variables.

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

Full Rust Installation with Cargo
----------------------------------

First install the Rust toolchain from [rust-lang.org/tools/install]()

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Local `git clone` installation

1. Clone this GitHub repository using [GitHub Desktop](https://github.com/apps/desktop) or the command line

    ```sh
    git clone https://github.com/elastic/esdiag.git
    ```

2. Install the `esdiag` tool using `cargo` from the local directory

    ```sh
    cargo install --path ~/GitHub/esdiag
    ```

    Where `~/GitHub/esdiag` is your local install directory. This will compile the `esdiag` tool and install it into your `~/.cargo/bin` directory created by the Rust toolchain.

3. Updates can be pulled from the GitHub repository and re-installed using the same command

    ```sh
    cd ~/GitHub/esdiag
    git pull
    cargo install --path .
    ```

    This will recompile the tool and install the new version.

### Alternative: install crate directly from GitHub

If you have `ssh` authentication already configured, it possible to install directly from GitHub.

1. Ensure your GitHub `ssh` credentials are working from the command line. If you haven't set this up yet, follow the [GitHub guide](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/adding-a-new-ssh-key-to-your-github-account)

    ```sh
    ssh -T git@github.com
    ```

    If it works, you will see this confirmation message:

    ```
    Hi ${username}! You've successfully authenticated, but GitHub does not provide shell access.
    ```

2. Install the crate (package) directly from the private GitHub using the `ssh` URL

    ```sh
    cargo install --git ssh://git@github.com/elastic/esdiag.git
    ```

    This automatically kicks off the build process. Ignore any warnings, report any errors.

3. Updates with this method use `cargo install esdiag` without needing to `git pull` first.

### Use it!

Validate the installation is working by simply running `esdiag help`. If you see the help message, you're ready to configure some hosts, setup a cluster, and import some diagnostics!

If you need a simple, local, security-disabled Elasticsearch and Kibana environment, use the `docker-compose.yml` file in the `docker` directory.

```sh
cd docker
docker compose up -d
```

This will download the latest Elasticsearch and Kibana images, start them up, and expose the ports `9200` and `5601` on your local machine.

Usage
--------------------

### Examples

1. Save a target Elasticsearch cluster to the hosts configuration
    ```sh
    esdiag host my_cluster elasticsearch http://localhost:9200
    ```

2. Setup the Elasticsearch cluster with the templates, data streams, etc.
    ```sh
    esdiag setup my_cluster
    ```

3. Process a diagnostic bundle from a local directory to `my_cluster`
    ```sh
    esdiag process ~/downloads/api-diagnostic-20240506-0050225 my_cluster
    ```

4. Open Kibana and explore!

### Commands

#### Help

`esdiag help` - Prints out the latest commands and usage guides
`esdiag help <command>` or `esdiag <command> --help` - prints out specific help for each subcommand.

```
Elastic Stack Diagnostics (esdiag) - collect diagnostics and import into Elasticsearch

Usage: esdiag <COMMAND>

Commands:
  collect  Collect a diagnostic bundle from a known host's API endpoints, writes output to a directory
  host     Configure and test a remote host connection
  import   [DEPRECATED] Process, enrich and import a diagnostic into Elasticsearch
  process  Receives a diagnostic from the input, processes it, and sends processed docs to the output
  setup    Import assets (templates, ingest pipelines, etc.) to a known Elasticsearch host
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

#### Host

The `esdiag host` command allows you configure and test authentication information. On a succesful connection test, it writes the configuration to your `~/esdiag/hosts.yml` file for easy re-use.

```
Configure, test and save a remote host connection to `~/.esdiag/hosts.yml`

Usage: esdiag host [OPTIONS] <NAME> [APP] [URL]

Arguments:
  <NAME>  A name to identify this host
  [APP]   Application of this host (elasticsearch, kibana, logstash, etc.)
  [URL]   A host URL to connect to

Options:
      --accept-invalid-certs  Accept invalid certificates
  -a, --apikey <APIKEY>       ApiKey, passed as http header
  -c, --cloud-id <CLOUD_ID>   Elastic Cloud ID (optional)
  -u, --username <USERNAME>   Username for authentication
  -p, --password <PASSWORD>   Password for authentication
  -n, --nosave                Don't save the host configuration on succesful connection
  -h, --help                  Print help
```

#### Setup

You must setup a host to use the `esdiag setup` command. It will send the required index templates and other assets into your Elasticsearch cluster. This may be either a pre-configured known host, or use the `ESDIAG_OUTPUT_*` environment variables.

```
Import assets (templates, ingest pipelines, etc.) to a known Elasticsearch host

Usage: esdiag setup [HOST]

Arguments:
  [HOST]  Known Elasticsearch host to import assets into; if omitted the ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, ESDIAG_OUTPUT_PASSWORD variables will be checked.

Options:
  -h, --help  Print help
```

#### Import

> ⚠️ DEPRECATED: This command will be removed in a future version.

#### Process

The `esdiag process <input> [output]` will read the diagnostic data from `<input>`, run the source documents through a series of processors, and send the enriched documents to the `<output>` target.

The `<input>` may be:
    1. Archive file - a `.zip` output from the [support diangostic](https://github.com/elastic/support-diagnostics) tool
    2. Directory - the uncompressed directory from an archive
    3. Known host - saved in the `hosts.yml`
    4. Elastic Uploader link - A url with auth token formated as `https://token:0123456789@upload.elastic.co/d/abcdefghijklmnopqrstuvwxyz`

The optional `[output]` may be:
    1. Known host - Must be an Elasticsearch host saved in the `hosts.yml`
    2. File - writes in an `.ndjson` format
    3. `stdout` - use `-` as the output name
    4. Omitted - Uses values read from `ESDIAG_OUTPUT_*` environment variables

```
Receives a diagnostic from the input, processes it, and sends processed docs to the output

Usage: esdiag process <INPUT> [OUTPUT]

Arguments:
  <INPUT>
          Source to read diagnostic data from (archive, directory, known host or uploader URL)

  [OUTPUT]
          Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the target will be determined based on the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD.

Options:
  -h, --help
          Print help (see a summary with '-h')
```

Once you have known hosts configured, you can add a simple shell commands shortcuts to your `~/.bashrc` or `~/.zshrc`. For example if you have `diag-cluster` as a known host:

```sh
esd() { esdiag process $1 diag-cluster }
```

Allows you to process diagnostics into the remote cluster with only:

```sh
esd ~/Downloads/api-diagnostic-20240506-0050225.zip
```

And a second function for an Elasticsearch cluster on your local machine, with `localhost` as a configured known host:

```sh
esdl() { esdiag process $1 localhost }
```

To pull a diagnostic into your local cluster directly from `diag-cluster`:

```sh
esdl diag-cluster
```

#### Collect

The `esdiag collect` command pulls the minimum required diagnostics from an Elasticsearch host and saves them to a directory. These are JSON-only, not pretty-printed, and do not include human-readable metrics. This bunlde captures only what is needed to then import with `esdiag`.

Authentication must be setup in advance with the `esdiag host` command or `hosts.yml` file. Direct access to the clsuter is required, this cannot be done through any Elastic Cloud API.

```
Collect a diagnostic bundle from a known host's API endpoints, writes output to a directory

Usage: esdiag collect <HOST> <OUTPUT>

Arguments:
  <HOST>    The Elasticsearch host to collect diagnostics from
  <OUTPUT>  An existing directory to create a diagnostic directory and files in

Options:
  -h, --help  Print help
```

### Debugging

Use a shell environment variables to enable debug logging:

```sh
export LOG_LEVEL=debug
```

This will enable debug-level log messages and when processing a diagnostic, `esdiag` will write debugging files into an `~/.esdiag/last_run` directory:

1. `metadata.ndjson` - This contains the diagnostic metadata and lookup tables generated while processing the diagnostic.
2. `responses.ndjson` - This contains all the HTTP responses from the Elasticsearch `_bulk` API.
3. `errors.ndjson` - Only the errors from the `_bulk` API, very useful when tracking down specific document errors.
