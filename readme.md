Elastic Stack Diagnostics
==============================

The Elastic Stack Diagnostics (`esdiag`) tool simplifies processing and importing diagnostic bundles into Elasticsearch. By pre-processing, splitting, and enriching the raw API outputs, building Kibana dashboards and ES|QL queries on diagnostic data is easy.

Installation
--------------------

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
    esdiag host my_cluster elasticsearch http://localhost:9200 --auth None --save
    ```


2. Setup the Elasticsearch cluster with the templates, data streams, etc.
    ```sh
    esdiag setup my_cluster
    ```

3. Import a diagnostic bundle from a local directory
    ```sh
    esdiag import my_cluster ~/downloads/api-diagnostic-20240506-0050225
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
  collect  [NOT IMPLEMENTED] Collects diagnostics from a host's API endpoints
  import   Process, enrich and import a diagnostic into Elasticsearch
  host     Configure and test a remote host connection
  setup    Setup required assets to visualize diagnostic imports
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

#### Host

The `esdiag host` command allows you configure and test authentication information, then save it to your `~/esdiag/hosts.yml` file for easy re-use.

```
Configure and test a remote host connection

Usage: esdiag host [OPTIONS] <NAME> [APP] [URL]

Arguments:
  <NAME>  A name to identify this host
  [APP]   Application of this host (elasticsearch, kibana, logstash, etc.)
  [URL]   A host URL to connect to

Options:
      --auth <AUTH>          Authentication method to use (none, basic, apikey, etc.) [default: none]
  -a, --apikey <APIKEY>      ApiKey, passed as http header
  -c, --cloud-id <CLOUD_ID>  Elastic Cloud ID (optional)
  -u, --username <USERNAME>  Username for authentication
  -p, --password <PASSWORD>  Password for authentication
  -s, --save                 Save the host configuration
  -h, --help                 Print help
```

#### Setup

The `esdiag setup` command will send all the required index templates and other assets into the target host, this must be an Elasticsearch cluster!

```
Setup required assets to visualize diagnostic imports

Usage: esdiag setup <HOST>

Arguments:
  <HOST>  Host to setup assets in

Options:
  -h, --help  Print help
```

#### Import

The `esdiag import` command allows these `target` and `source` options:

`target`
    1. stdout (use `-` as the target name)
    2. directory (the root directory of a diagnostic bundle)
    3. host (a known host saved to your `hosts.yml`)

`source`
    1. directory
```
Process, enrich and import a diagnostic into Elasticsearch

Usage: esdiag import [OPTIONS] <TARGET> <SOURCE>

Arguments:
  <TARGET>  Target to write processed diagnostic documents to (`-` for stdout)
  <SOURCE>  Source to read diagnostic data from

Options:
  -p, --pretty  Pretty print JSON
  -h, --help    Print help
```

#### Collect

🚧 This command is not yet implemented! 🚧

### Debugging

Use a shell environment variables to enable debug logging:

```sh
export LOG_LEVEL=debug
```

This will enable debug-level messages. Also, when you import a diagnostic `esdiag` will write two new files in your `~/.esdiag` directory:

1. `metadata.ndjson` - This contains the diagnostic metadata and lookup tables generated while processing the diagnostic.
2. `responses.ndjson` - This contains all the HTTP responses from the Elasticsearch `_bulk` API.
3. `errors.ndjson` - Only the errors from the `_bulk` API, very useful when tracking down specific document errors.
