Elastic Stack Diagnostics
==============================

The Elastic Stack Diagnostics (`esdiag`) tool simplifies processing and importing diagnostic bundles into Elasticsearch. By pre-processing, splitting, and enriching the raw API outputs, building Kibana dashboards and ES|QL queries on diagnostic data is easy.

Installation
--------------------
1. Install the Rust toolchain from [rust-lang.org/tools/install]()
    ```
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```

2. Ensure your GitHub `ssh` credentials are working from the command line
    ```
    ssh -T git@github.com
    ```
    
3. Install the crate (package) from GitHub using the `ssh` URL
    ```
    cargo install --git ssh://git@github.com/elastic/esdiag.git
    ```
    
4. Use it!

Usage
--------------------

### Examples

1. Save a target Elasticsearch cluster to the hosts configuration
    `esdiag host my_cluster elasticsearch http://localhost:9200 --auth None --save`
    
2. Setup the Elasticsearch cluster with the templates, data streams, etc.
    `esdiag setup my_cluster`
    
3. Import a diagnostic bundle from a local directory
    `esdiag import my_cluster api-diagnostic-20240506-0050225`

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

- `target` 
    1. stdout (use `-` as the target name)
    2. directory (the root directory of a diagnostic bundle)
    3. host (a known host saved to your `hosts.yml`)
- `source` 
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

Use environment variables to enable debug logging: `export LOG_LEVEL=debug`. This will enable debug-level messages and, when you import a diagnostic, writes two new files in your `~/.esdiag` directory:

1. `metadata.ndjson` - This contains the diagnostic metadata and lookup tables generated while processing the diagnostic.
2. `responses.ndjson` - This contains all the HTTP responses from the Elasticsearch output. Very useful when tracking down specific document errors returned from the `_bulk` API. 