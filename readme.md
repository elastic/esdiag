Elastic Stack Diagnostics
==========================

Elastic Stack Diagnostics (`esdiag`) simplifies processing and importing diagnostic bundles into Elasticsearch. It pre-processes, splits and enriches the raw API outputs into Elasticsearch-friendly JSON documents. This makes using diagnostic data for Kibana dashboards, ES|QL queries, and more, easy.

Running locally within containers
----------------------------------

### 1. Preparation

Use the `bin/esdiag-control` command to quickly spin up a fully-local environment.

1. Clone this repository to your local machine using either `git` or [GitHub Desktop](https://desktop.github.com/download/)
2. Install the `esdiag-control` dependencies: `docker`, `jq`, `curl`, `grep`, and `sed`.
3. Have either `podman` or `docker` container runtime with `compose` subcommand support.
4. Have at least 8GB of total RAM available for the containers

> [!IMPORTANT]
> By default containers running on Linux can typically access the host's total available memory, so the 8GB requirement applies to the host machine. On MacOS and Windows the containers run inside a virtual machine that commonly has less than 8GB RAM by default. Both the Docker and Podman Desktop apps have a `resources` section to configure it. Podman also has a command-line option: `podman machine set --cpus 8 --memory 8192`

### 2. Running

Run the script from this repository's root directory:

```sh
./bin/esdiag-control up
```

> [!TIP]
> When running security enabled, the `elastic` user's password will be saved to the `ELASTIC_PASSWORD` environment variable in the `.env` file. It will be printed last, before the browser is launched.

or with security disabled:

```sh
./bin/esdiag-control up --insecure
```

> [!NOTE]
> The AI assistant features will not be available with security disabled. Running with security disabled prevents Kibana from using an Kibana encryption key, which is required to configure anything with an external API key, like large-language model (LLM) providers.

Once the script is complete, you will have:
1. A single Elasticsearch node with all index templates installed.
2. A fully-configured Kibana instance with dashboards, data views, and saved searches imported.
3. An `esdiag:latest` container serving the web interface.
4. A web browser opened to the ESDiag web interface at `http://localhost:2501`

### 3. Processing diagnostics

Open your browser to the ESDiag web interface at `http://localhost:2501` and use your browser to upload a diagnostic bundle. The first time you open Kibana, use the `elastic` username and password printed to your terminal (also saved in the `.env` file).

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

Refer to the `example.env` file to configure a default output with environment variables, without any `host` configurations.

If you need a simple way to run the full stack locally, including Elasticsearch and Kibana, use the `bin/esdiag-local` script above. You can still target the containers with a local ESDiag install, just be sure to stop the `esdiag` container before trying to run `esdiag serve`.

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

If you set the `ESDIAG_KIBANA_URL` environment variable with your target Kibana URL (no trailing `/`), ESDiag will log a link directly to a pre-filtered cluster report dashboard.

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
  keystore Manage encrypted secrets in the local keystore
  process  Receives a diagnostic from the input, processes it, and sends processed docs to the output
  serve    Start a web server to receive diagnostic bundle uploads
  setup    Import assets (templates, ingest pipelines, etc.) to a known Elasticsearch host
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

#### Host

The `esdiag host` command allows you configure and test authentication information. On a succesful connection test, it writes the configuration to your `~/.esdiag/hosts.yml` file for easy re-use.

Alternatively you can use a `.env` file and set `ESDIAG_OUTPUT_*` values; see `example.env`.

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
  -u, --user <USERNAME>       Username for authentication (alias: --username)
  -p, --password <PASSWORD>   Password for authentication
      --secret <SECRET>       Secret identifier in the encrypted keystore
      --roles <ROLES>         Comma-separated host roles (collect,send,view)
  -n, --nosave                Don't save the host configuration on succesful connection
  -h, --help                  Print help
```

Examples:

```sh
# Host backed by a keystore secret reference
esdiag host prod-es elasticsearch http://localhost:9200 --secret prod-es-apikey

# Host with explicit roles for workflow filtering
esdiag host prod-es elasticsearch http://localhost:9200 --roles collect,send
```

#### Keystore

The `esdiag keystore` command manages encrypted local secrets used by `--secret` references in `hosts.yml`.

```
Manage encrypted secrets in the local keystore

Usage: esdiag keystore <COMMAND>

Commands:
  add <SECRET_ID>       Add or update a secret in the encrypted keystore
  remove <SECRET_ID>    Remove a secret from the encrypted keystore
  migrate               Migrate legacy host credentials in hosts.yml into the keystore
```

Examples:

```sh
# Add a basic auth secret
esdiag keystore add prod-es-basic --user elastic --password changeme

# Add an API key secret
esdiag keystore add prod-es-apikey --apikey BASE64_ENCODED_KEY

# Remove just the API key auth from a secret
esdiag keystore remove prod-es-apikey --apikey BASE64_ENCODED_KEY

# Move plaintext hosts.yml credentials into keystore entries
esdiag keystore migrate
```

Use `ESDIAG_KEYSTORE_PASSWORD` to provide the keystore password non-interactively. In interactive shells, `keystore add/remove` will prompt when it is unset.

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
          Source to read diagnostic data from (archive, directory, known host, or uploader URL)

  [OUTPUT]
          Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the target will be determined based on the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD.

Options:
  -a, --account <ACCOUNT>
          Diagnostic report account name

      --debug
          Enable debug logging

  -c, --case <CASE>
          Diagnostic report case number

  -o, --opportunity <OPPORTUNITY>
          Diagnostic report opportunity

  -u, --user <USER>
          Diagnostic report user

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

#### Serve

> [!NOTE]
> This is the default entry command when run from a container.

The `esdiag serve` command starts a web server that accepts diagnostic bundle uploads through a user-friendly interface. This makes it easy to receive and process diagnostics without requiring command-line access from the uploading user.

```
Start a web server to receive diagnostic bundle uploads

Usage: esdiag serve [OPTIONS] [OUTPUT]

Arguments:
  [OUTPUT]
          Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the output will try using the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD.

Options:
  -p, --port <PORT>
          The port to bind the server to [default: 2501]
  -h, --help
          Print help
```

Example usage:

```sh
# Start a server on the default port 2501 that sends processed diagnostics to a known host
esdiag serve localhost

# Start a server on port 8080
esdiag serve --port 8080 localhost
```

You can access the web interface at http://localhost:2501 (or your specified port) or use curl to upload a file:

```sh
curl -F "file=@/path/to/diagnostic.zip" http://localhost:2501/upload
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
