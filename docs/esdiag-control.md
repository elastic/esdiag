# Elastic Stack Diagnostics Control

The `esdiag-control` script helps build, configure and deploys Elastic Stack Diagnostics (ESDiag) to a target environment.

You can use either Podman (preferred) or Docker to build and run ESDiag along the Elastic Stack inside containers.

## Quickstart: MacOS Installation with Homebrew and Podman

1. Install [Homebrew package manager](https://brew.sh/)
  ```bash
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  ```

2. Install Podman and `esdiag-control` dependencies
  ```bash
  brew install podman podman-compose jq
  ```

3. Setup Podman machine, increase it's resources, and start it
  ```sh
  podman machine init
  podman machine set --cpus 8 --memory 8192
  podman machine start
  ```

4. Create the minimal `.env` file to pull dashboards with a [GitHub personal access token](github-token.md)
  ```sh
  export GITHUB_TOKEN=<your_github_token>
  ```

5. Launch a full security-disabled Elastic Stack with the ESDiag web interface
  ```sh
  ./bin/esdiag-control launch --insecure
  ```

## Examples

Build a container image for the current host's platform
```sh
esdiag-control build
```

Build a multi-platform container image, pushing it to the container registry
```sh
esdiag-control buildx --push
```

Generate a `target/esdiag-compose.yml` compose file for a full Elastic Stack
```sh
esdiag-control compose
```

Generate a `target/esdiag-compose.yml` compose file, with security disabled, and open a browser to it
```sh
esdiag-control launch --insecure
```

Setup an existing stack monitoring cluster with ESDiag assets
```sh
export ESDIAG_OUTPUT_URL="https://elasticsearch.example.com"
export ESDIAG_OUTPUT_APIKEY="abcdefghijklmnopqrstuvwxyz"
esdiag-control setup
```

Build the multi-platform container image, push it to your repository. You will need to pre-configure authentication in `registries.conf` and use `podman login` or `docker login`.
```sh
export ESDIAG_REGISTRY="registry.example.co"
esdiag-control buildx --push
```

## Commands

### help

`esdiag-control help` - Prints out the latest commands and usage guides
`esdiag-control help <command>` or `esdiag-control <command> --help` - prints out specific help for each subcommand.

```
Description:
    Elastic Stack Diagnostics Control esdiag-control is the deployment assistant for ESDiag

Usage:
    esdiag-control <command> [options] <arguments>

Commands:
    auth     Test the authentication for ESDIAG_OUTPUT_URL and ESDIAG_KIBANA_URL
    build    Build an ESDiag container image for the local host's platform
    buildx   Build a multi-platform ESDiag container image with buildx
    compose  Generate a full Elastic Stack deployment using Docker compose
    launch   Generate and launch a full Elastic Stack deployment using Docker compose
    setup    Setup the target Elasticsearch and Kibana instances with ESDiag assets
    help     To get detailed <command> help, use a command name as the <argument>

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE file to source credentials from (default .env)
    -s, --space <ID>       - Kibana space id (default esdiag)
        --debug            - More verbose logging and retention of temporary files
        --version          - Print the version of the script
```

### auth

```
Command: auth
    Test the configured authorization for Elasticsearch and Kibana

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE file to source credentials from (default .env)

Environment Variables:
    ESDIAG_KIBANA_URL          - Kibana URL (default http://localhost:5601)
    ESDIAG_OUTPUT_URL          - Elasticsearch URL (default http://localhost:9200)
    ESDIAG_OUTPUT_APIKEY       - Elasticsearch API key, takes precedence over username/password
    ESDIAG_OUTPUT_USERNAME     - Elasticsearch username
    ESDIAG_OUTPUT_PASSWORD     - Elasticsearch password
```

### build

```
Command: build
    Build an ESDiag container image for the local host's platform

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE to source credentials from (default .env)
    -r, --registry <URL>   - Elastic container registry URL
    -p, --push             - Push the container image to the registry

Environment Variables:
    ESDIAG_REGISTRY - Private container registry to publish to
```

### buildx

```
Command: buildx
    Build a multi-platform ESDiag container image for x86_64 and arm64

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE to source credentials from (default .env)
    -r, --registry <URL>   - Elastic container registry URL
    -p, --push             - Push the container image to the registry

Environment Variables:
    ESDIAG_REGISTRY - Private container registry to publish to
```

### compose

```
Command: compose [input_file] [output_file]
    Generate a full Elastic Stack deployment for use with docker compose

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE to source credentials from (default .env)
    -i, --insecure         - Setup the Elasticsearch cluster with security disabled
    -r, --registry <URL>   - Elastic container registry URL

Arguments:
    [input_file]           - Compose file to read in (default docker/docker-compose.yml)
    [output_file]          - Compose file to save out (default target/esdiag-compose.yml)

Environment Variables:
    ELASTIC_CONTAINER_REGISTRY - Elastic container registry (default docker.elastic.co)
    ESDIAG_REGISTRY            - Private container registry (default $ELASTIC_CONTAINER_REGISTRY)
```

### launch

```
Command: launch [input_file] [output_file]
    Generate, launch, and setup a full Elastic Stack deployment with docker compose up -d

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE to source credentials from (default .env)
    -i, --insecure         - Setup the Elasticsearch cluster with security disabled
    -r, --registry <URL>   - Elastic container registry URL
    -s, --space            - Kibana space id (default esdiag)

Arguments:
    [input_file]           - Compose file to read in (default docker/docker-compose.yml)
    [output_file]          - Compose file to save out (default target/esdiag-compose.yml)

Environment Variables:
    ELASTIC_CONTAINER_REGISTRY - Elastic container registry (default docker.elastic.co)
    ESDIAG_REGISTRY            - Private container registry (default $ELASTIC_CONTAINER_REGISTRY)
```

### setup

```
Command: setup
    Generate, launch, and setup a full Elastic Stack deployment with docker compose up -d

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE to source credentials from (default .env)
    -i, --insecure         - Setup the Elasticsearch cluster with security disabled
    -r, --registry <URL>   - Elastic container registry URL
    -s, --space            - Kibana space id (default esdiag)

Environment Variables:
    ELASTIC_CONTAINER_REGISTRY - Elastic container registry (default docker.elastic.co)
    ESDIAG_REGISTRY            - Private container registry (default $ELASTIC_CONTAINER_REGISTRY)
    ESDIAG_KIBANA_URL          - Kibana URL (default http://localhost:5601)
    ESDIAG_OUTPUT_URL          - Elasticsearch URL (default http://localhost:9200)
    ESDIAG_OUTPUT_APIKEY       - Elasticsearch API key , takes precedence over username/password
    ESDIAG_OUTPUT_USERNAME     - Elasticsearch username
    ESDIAG_OUTPUT_PASSWORD     - Elasticsearch password
```

## Troubleshooting and Errors

### podman/docker compose up failed with exit status 0

If you have used Docker compose with Docker Desktop on your machine in the past, you may see an error like this:
```sh
[2025-08-24 18:22:44 Info esdiag-control] Running podman compose up --detach
[2025-08-24 18:23:27 Error esdiag-control] podman compose up failed with exit status 0

podman compose --file target/esdiag-compose.yml up -d
>>>> Executing external compose provider "/opt/homebrew/bin/docker-compose". Please see podman-compose(1) for how to disable this message. <<<<

[+] Running 0/2
 ⠋ esdiag-kibana Pulling                                                                                                                                0.0s
 ⠋ esdiag-elasticsearch Pulling                                                                                                                         0.0s
error getting credentials - err: exec: "docker-credential-desktop": executable file not found in $PATH, out: ``
Error: executing /opt/homebrew/bin/docker-compose --file target/esdiag-compose.yml up -d: exit status 1
```

If so, delete (or rename) your `~/.docker` directory as an old configuration may be conflicting with Podman.
