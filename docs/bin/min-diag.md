
min-diag.sh
------------

A script to `collect` the minimum Elasticsearch diagnostic bundles required to
import into ESDiag; with a `watch` function to periodically collect at intervals.

As a portable bash script, it can be run on any system with bash installed. Authentication is handled through the `APIKEY` and `URL`  variables inside the script.

The `collect` command pulls one minimal diagnostic bundle from the cluster:

```bash
./min-diag.sh collect
```

Outputs one directory named `api-diagnostics-<timestamp>` with the diagnostic files in it.

The `watch` command periodically collects diagnostic bundles from the cluster:

```bash
./min-diag.sh watch
```

This outputs many directories named `api-diagnostics-<timestamp>`. The total number of collections, and the intervals between collections, are the `WAIT_TIME` and `COLLECTION_COUNT` variables inside the script.

Processing all of the diagnostic directories output by the `watch` command can be done with a single shell loop:

```bash
for DIR in api-diagnostic-*; do esdiag process $DIR localhost; done
```

Where `localhost` is a saved known host in the `~/.esdiag/hosts.yml` configuration file.
