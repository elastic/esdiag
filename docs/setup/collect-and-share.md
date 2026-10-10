---
type: Guide
title: Collect and share diagnostics
description: Collect a raw diagnostic archive and send it to Elastic Upload Service from the web UI or CLI.
tags: [setup, collection, upload, onboarding]
---

# Collect and share diagnostics

Use this guide when someone else will analyze the diagnostic. It creates a raw
ZIP archive and can send it to Elastic Upload Service. You do not need a
diagnostic cluster.

You can do every step in the web UI or the CLI. Both use the same saved hosts,
keystore, and jobs, so you can switch between them.

## What you need

- The [native binary](installation.md#native-binary).
- Network access to the Elasticsearch, Kibana, or Logstash endpoint.
- Credentials that can read its diagnostic APIs.
- A local directory with enough space for the archive.
- An Elastic Upload Service ID or URL if you will upload the archive.

Enter credentials only at ESDiag's masked prompts or password fields. Do not
put them in shell history, tickets, documents, or chat. Upload IDs and URLs are
not credentials, but they work like short-lived tokens: they expire, so ask for
a new one if yours has.

## Web UI

### Start the web UI

Run the initializer and accept the web interface handoff:

```sh
esdiag init
```

Enter the diagnostic user, preferably your email address, then answer `y` to
**Continue setup in the web interface?** ESDiag starts a local web server and
opens `http://127.0.0.1:2501/welcome` with that user filled in. The terminal
stays attached until you stop the server with Ctrl+C.

On the welcome page:

1. Under **Diagnostic Tasks**, choose **Collect**.
2. Create or unlock the keystore. The password is never stored in the browser
   or `esdiag.yml`.
3. Add the source you will collect from, with its credential.
4. Save the default job.

To start the web UI later without rerunning setup:

```sh
esdiag serve
```

Then open `http://127.0.0.1:2501`.

### Collect and upload

Open **Advanced** in the web UI. It shows the job as three cards: **Collect**,
**Process**, and **Send**.

1. In **Collect**, choose **New**, then **Known Host**. Select the source and a
   **Diagnostic Type**. `standard` is the usual choice.
2. Turn on **Download Archive** to keep a copy of the archive.
3. In **Process**, turn off **Enabled**. ESDiag forwards the raw archive without
   processing it.
4. In **Send**, choose one destination:
   - **Remote**, then enter the **Upload ID**, to send the archive to Elastic
     Upload Service.
   - **Local** to only download the archive.
5. Select **Collect**.

The job result shows the source and, for an upload, the destination. Your
browser downloads the archive when **Download Archive** is on.

For a one-off collection without saving the source, choose **API Key** instead
of **Known Host**. The web UI does not save or log that key.

### Upload an archive you already have

1. In **Collect**, choose **Existing**, then select or drop the archive.
2. In **Process**, turn off **Enabled**.
3. In **Send**, choose **Remote** and enter the **Upload ID**.
4. Select **Upload**.

## CLI

### Guided setup

Run the initializer at an interactive terminal:

```sh
esdiag init
```

Answer the prompts:

1. Enter the diagnostic user, preferably your email address.
2. Answer `n` to **Continue setup in the web interface?**
3. Choose **Only collect new diagnostics**.
4. Enter a name and URL for the Elasticsearch cluster you will collect from.
5. Choose how to supply its API key: read it from a file, read it from an
   environment variable, or paste it at a masked prompt.
6. Create or unlock the keystore when ESDiag asks.

ESDiag saves the host, stores the API key in the keystore, and saves a default
job named `<host>-collect`. It does not configure a diagnostic cluster or
install assets. Run `esdiag init` again to change any answer.

The initializer only adds Elasticsearch sources with an API key. For Kibana,
Logstash, or a username and password, use the manual steps below.

### Save a source

Create or unlock the encrypted keystore:

```sh
esdiag keystore unlock
```

Add the source credential. Omit the API key or password value so ESDiag prompts
for it:

```sh
esdiag keystore add source-cluster --apikey
```

```sh
esdiag keystore add source-cluster --user elastic --password
```

Save and test the source:

```sh
esdiag host add source-cluster https://es.example.com:9200
esdiag host auth source-cluster
```

Use the real endpoint, including its base path. Keep certificate verification
enabled unless the endpoint owner approves an exception. ESDiag infers the app
from ports 9200, 5601, and 9600. On any other port, add
`--app elasticsearch`, `kibana`, or `logstash`.

### Collect

The output directory must exist:

```sh
mkdir -p "$HOME/diagnostics"
esdiag collect source-cluster "$HOME/diagnostics"
```

The command reports the archive path. Check that the file exists before you
share it.

The collection level defaults to `standard`. Use `--type minimal`, `light`, or
`support` to change it.

If `init` saved a job, run it instead:

```sh
esdiag job list
esdiag job run <NAME>
```

### Upload

To upload immediately after collection:

```sh
esdiag collect source-cluster "$HOME/diagnostics" --upload '<UPLOAD_ID_OR_URL>'
```

To upload an archive you already have:

```sh
esdiag upload /path/to/diagnostic.zip '<UPLOAD_ID_OR_URL>'
```

The collected archive stays on disk. Use your approved method to keep upload
IDs and URLs out of shell history.

## Next

To analyze the archive yourself, set up a diagnostic cluster. The source you
saved here works for direct collection in every guide:

- [Run a local stack](local-stack.md)
- [Use a remote cluster](remote-cluster.md)
- [Use a Serverless project](serverless.md)
