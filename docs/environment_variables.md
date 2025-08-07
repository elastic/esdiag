# Environment Variables

The is a list of environment variables used by ESDiag. Any of these can be defined in the `docker-compose.yml` or for running in a local shell.

## General

-   `HOME`: On Linux and macOS, specifies the user's home directory. This is used to determine the location for storing files like known hosts.
-   `USERPROFILE`: On Windows, specifies the user's home directory. This is the equivalent of `HOME` for Windows systems.
-   `ESDIAG_KIBANA_URL`: The base URL for a Kibana instance. This is used to generate links to pre-filtered dashboards. This needs to be a Kibana instance attached to the output Elasticsearch cluster.

## Exporter

These variables configure the data exporter, which is responsible for sending diagnostic data to a remote endpoint.

-   `ESDIAG_OUTPUT_URL`: The URL to send the output documents to. This is used if no output is specified via command-line arguments.
-   `ESDIAG_OUTPUT_APIKEY`: The API key for authenticating with the output endpoint.
-   `ESDIAG_OUTPUT_USERNAME`: The username for basic authentication with the output endpoint.
-   `ESDIAG_OUTPUT_PASSWORD`: The password for basic authentication with the output endpoint.

If both are defined, the API key takes precedence.

## Server

These variables configure the `esdiag serve` command's web interface.

-   `ESDIAG_PORT`: The network port on which the server will listen for incoming requests. It defaults to `3000` if not specified.
-   `ESDIAG_USER`: Specifies a user email, which can be used for identification or authentication purposes as an alternative to the `X-Goog-Authenticated-User-Email` header.
