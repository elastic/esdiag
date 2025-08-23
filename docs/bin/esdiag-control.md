# Elastic Stack Diagnostics Deploy

The `esdiag-deploy.sh` script deploys Elastic Stack Diagnostics (ESDiag) to a target environment.

## Usage

```sh
esdiag-deploy.sh [command] [options]
```

### Options

-r, --registry <url>   Elastic container registry URL
-e, --env              The `.env` environment file

--help                Print this help message
--version             Print the version of the script
--debug               Enable debug logging

### Commands

build                 Build an ESDiag container image for the local platform

-p, --push             Push the container image to the registry

buildx                Build a multi-platform ESDiag container image with buildx

-p, --push             Push the container image to the registry

compose               Generate a full Elastic Stack deployment using Docker compose

-i, --insecure         Setup the Elasticsearch cluster with security disabled
-f, --file             Use or create the specified `docker-compose.yml` file

launch                Compose, setup and launch a full Elastic Stack deployment using Docker compose

-i, --insecure         Setup the Elasticsearch cluster with security disabled
-f, --file             Use or create the specified `docker-compose.yml` file
-s, --space            Kibana space id to use (default "esdiag")

setup                 Setup the target Elasticsearch and Kibana instances with ESDiag assets

-s, --space            Kibana space id to use (default "esdiag")

## Examples

Build a container image for the current host's platform
```sh
esdiag-control build
```

Build a multi-platform container image, pushing it to the container registry
```sh
esdiag-control buildx --push
```

Generate a Docker Compose file for a full Elastic Stack
```sh
esdiag-control compose
```

Generate a Docker compose file for a full Elastic stack, with security disabled, and launch it
```sh
esdiag-control launch --insecure
```

Setup an existing stack monitoring cluster with ESDiag assets
```sh
esdiag-control setup
```

Build the multi-platform container image, push it to your repository, and setup an existing stack monitoring cluster with ESDiag assets
```sh
esdiag-control buildx --push
esdiag-control setup
```
