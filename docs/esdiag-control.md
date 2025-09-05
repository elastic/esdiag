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

5. Start a full security-disabled Elastic Stack with the ESDiag web interface
  ```sh
  ./bin/esdiag-control up --insecure
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

Configure and start up a full Elastic Stack deployment, with security disabled, and open a browser to it
```sh
esdiag-control up --insecure
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
    up       Configure and start up a full Elastic Stack deployment using compose
    setup    Setup the target Elasticsearch and Kibana instances with ESDiag assets
    down     Bring down the Elasticsearch, Kibana and ESDiag containers
    help     To get detailed <command> help, use a command name as the <argument>

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE file to source credentials from (default .env)
    -s, --space <ID>       - Kibana space id (default esdiag)
        --debug            - More verbose logging and retention of temporary files
        --version          - Print the version of the script
```

### up

```
Command: up [options]
    Configure and start up a full Elastic Stack deployment with podman compose up -d

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE to source credentials from (default .env)
    -i, --insecure         - Setup the Elasticsearch cluster with security disabled
    -r, --registry <URL>   - Elastic container registry URL
    -s, --space            - Kibana space id (default esdiag)

Environment Variables:
    ELASTIC_CONTAINER_REGISTRY - Elastic container registry (default docker.elastic.co)
    ESDIAG_REGISTRY            - Private container registry (default localhost)
```

### down

```
Command: down
    Remove all containers with podman compose down, optionally also delete the volume

Options:
    --remove-file          - Also remove the currently-configured target/docker-compose.yml file
    --remove-image         - Also remove the ESDiag image, will require re-building or re-downloading for a new container
    --remove-volume        - Also remove the volume WARNING: Permanently deletes all data from the cluster and invalidates security configuration!
    --remove-all           - Remove the containers, image, volume, and compose file
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
    ESDIAG_REGISTRY        - Private container registry to publish to (default localhost)
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
    ESDIAG_REGISTRY        - Private container registry to publish to (default localhost)
```

### setup

```
Command: setup
    Configure and start up a full Elastic Stack deployment with compose up -d

Options:
    -e, --env <NAME|FILE>  - The .env.NAME or FILE to source credentials from (default .env)
    -i, --insecure         - Setup the Elasticsearch cluster with security disabled
    -r, --registry <URL>   - Elastic container registry URL
    -s, --space            - Kibana space id (default esdiag)

Environment Variables:
    ELASTIC_CONTAINER_REGISTRY - Elastic container registry (default docker.elastic.co)
    ESDIAG_REGISTRY            - Private container registry (default localhost)
    ESDIAG_KIBANA_URL          - Kibana URL (default http://localhost:5601)
    ESDIAG_OUTPUT_URL          - Elasticsearch URL (default http://localhost:9200)
    ESDIAG_OUTPUT_APIKEY       - Elasticsearch API key , takes precedence over username/password
    ESDIAG_OUTPUT_USERNAME     - Elasticsearch username
    ESDIAG_OUTPUT_PASSWORD     - Elasticsearch password
```

## Troubleshooting and Errors

### podman/docker compose up failed with exit status 0

If you have used Docker compose with Docker Desktop on your machine in the past, you may see an error like this:
```
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
