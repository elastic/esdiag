---
type: Guide
title: Use a remote cluster
description: Process and analyze diagnostics in an existing self-managed or Elastic Cloud Hosted deployment.
tags: [setup, elasticsearch, exporter, hosts]
---

# Use a remote cluster

Use this guide when processed diagnostics belong in an Elasticsearch deployment
that already exists, such as a self-managed cluster or an Elastic Cloud Hosted
deployment. For an Elasticsearch Serverless project, see
[Use a Serverless project](serverless.md).

## What you need

- Network access to the Elasticsearch endpoint.
- A credential that can write `*-esdiag` indices.
- An administrator credential to install ESDiag assets when they are missing.
- The matching Kibana URL and a configured inference model if you need
  dashboards or Agent Builder.

## Install

| Install | Gives you | Choose it when |
|---|---|---|
| [Native binary](installation.md#native-binary) | CLI, web UI, saved jobs, and the coding-agent skill | You work from your own machine. This is the default. |
| [Container image](installation.md#container-image) | The web UI only | You want to run ESDiag in a container. |

To give a team one web UI with central credentials, run the container in
service mode. See [Use a shared service](shared-service.md#service-administrators).

## Connect the cluster

### Native: guided

Run the initializer at an interactive terminal:

```sh
esdiag init
```

Choose processing, then choose **remote**. The initializer saves the
Elasticsearch destination, the linked Kibana viewer, encrypted credentials, and
the default output. It also offers to install assets and configure collection.

If validation fails, `init` names the endpoint and lets you re-enter its
settings. Declining to replace an existing output returns to output selection
without changing the saved deployment. API key source numbers depend on which
sources are available. Use the displayed number or type `paste`, `file`, or
`env`.

To answer the same questions in the browser, answer `y` to **Continue setup in
the web interface?** ESDiag opens `http://127.0.0.1:2501/welcome`. API keys use
masked form fields and are never stored in browser state or `esdiag.yml`.

### Native: manual

Use these steps when the cluster administrator and the ESDiag user have
separate responsibilities.

```sh
esdiag keystore unlock
esdiag keystore add diagnostics-output --apikey
esdiag host add diagnostics-output https://diagnostics.example.com:9200 --roles send
esdiag host auth diagnostics-output
```

Do not use a raw HTTP URL as a `process` output. Save it as a host so ESDiag can
validate the role and resolve its credentials.

### Container

Save these lines as `esdiag.env` with your editor, then restrict the file to
your user:

```text
ESDIAG_OUTPUT_URL=https://diagnostics.example.com:9200
ESDIAG_OUTPUT_APIKEY=<API_KEY>
ESDIAG_KIBANA_URL=https://kibana.example.com/s/esdiag
```

```sh
chmod 600 esdiag.env
```

The container takes its diagnostic cluster from these variables. Its web
onboarding validates both endpoints and checks the ESDiag assets, so it does
not ask for another cluster.

## Install assets

Elasticsearch needs ESDiag templates and ingest pipelines. Kibana needs
dashboards and Agent Builder assets. Run setup with a credential that can
install them.

Native:

```sh
esdiag setup diagnostics-output
```

Container:

```sh
docker run --rm --env-file esdiag.env \
  docker.elastic.co/esdiag/esdiag:<version> setup
```

`esdiag setup <saved-host>` prepares Elasticsearch. Run `esdiag setup` against
the saved Kibana host to install the Kibana assets separately, or let
`esdiag init` install both.

If normal ingestion uses a less-privileged credential, replace the saved secret
after setup:

```sh
esdiag keystore update diagnostics-output --apikey
esdiag host auth diagnostics-output
```

### Kibana space

Kibana assets install into the `esdiag` space unless `ESDIAG_KIBANA_SPACE`
selects another one. To install into Kibana's default space:

```sh
ESDIAG_KIBANA_SPACE=_default esdiag setup <saved-kibana-host>
```

`_default` skips space creation and omits `/s/{space}` from asset requests and
links. An empty value or Kibana's `default` ID also selects the default space.
A named value such as `support` installs into that space. An explicit selection
overrides a space prefix in the configured Kibana URL. Keep the same setting
when running `process`, `serve`, or `agent ask` so their links use that space.

Install ESDiag assets into only one Kibana space. If they already exist in
another space, setup stops before importing and names that space. Set
`ESDIAG_KIBANA_SPACE` to that space to update it, or remove the assets from it
before installing elsewhere.

## Process and analyze an archive

Native:

```sh
esdiag process /path/to/diagnostic.zip diagnostics-output
```

If `esdiag init` made this cluster the default output, omit its name:

```sh
esdiag process /path/to/diagnostic.zip
```

If the Kibana viewer and Agent Builder are ready, ask a question as part of
processing:

```sh
esdiag process /path/to/diagnostic.zip \
  --ask "What is the highest-risk finding, and what evidence supports it?"
```

The result includes a diagnostic ID and a Kibana conversation URL.

Container: start the web UI, then upload the archive on the **Process** page at
`http://127.0.0.1:2501`.

```sh
docker run -d --name esdiag --env-file esdiag.env \
  -p 127.0.0.1:2501:2501 -v esdiag-data:/root/.esdiag \
  docker.elastic.co/esdiag/esdiag:<version> serve
```

The `esdiag-data` volume keeps the container's saved hosts, jobs, and keystore
across restarts.

## Collect, process, and analyze

Save the cluster you want to diagnose as a `collect` host. Use the web UI's
**Settings** page, or follow [Save a source](collect-and-share.md#save-a-source).

Native: run the job that `esdiag init` saved.

```sh
esdiag job list
esdiag job run <NAME>
```

Web UI: open **Advanced**. In **Collect**, choose **New**, then **Known Host**.
Leave **Process** enabled, choose the diagnostic cluster in **Send**, then
select **Collect**. Add an **Elastic Upload Service ID** under **Raw bundle
delivery** to share the raw archive too.

## Maintain

Saved hosts, jobs, settings, and encrypted secrets live in `~/.esdiag` for the
user who runs `esdiag`. They are separate from local-stack state and from the
container's volume.

Rotate a credential:

```sh
esdiag keystore update diagnostics-output --apikey
```

To remove the output, first remove jobs and defaults that reference it:

```sh
esdiag host remove diagnostics-output
esdiag keystore remove diagnostics-output
```

For inference setup, see the [LLM configuration guide](../llm-setup-guide.md).
