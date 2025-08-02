The Logstash diagnostic only contains five `.json` files with relevant non-duplicate data.

```
 1. ✅ diagnostic_manifest.json
 2. ❌ diagnostics.log
 3. ❌ logstash_diagnostic_flow_metrics.html
 4. ✅ logstash_node.json
 5. ✅ logstash_node_stats.json
 6. ✅ logstash_nodes_hot_threads.json
 7. ❌ logstash_nodes_hot_threads_human.txt
 8. ✅ logstash_plugins.json
 9. ✅ logstash_version.json
10. ☑️ manifest.json
```

The `manifest.json` is only used as a fallback if `diagnostic_manifest.json` is not present.

#### logstash_version.json

```yaml
host: String,
version: SemVer,
http_address: IP:PORT,
id: String,
name: String,
ephemeral_id: String,
status: String,
snapshot: boolean,
pipeline: PipelineSettings,
build_date: Timestamp,
build_sha: String,
build_snapshot: boolean,
```

**Datastream:** _None_

The `logstash_version.json` is a good baseline for the document metadata.

Only the `build_*` properties are not present in the other files.

This can be nested inot the `logstash` property on other docs.

The remaining files only list top-level properties not capture in `logstash_version.json`.

#### logstash_node.json

```yaml
pipelines: HashMap<String, PipelineConfig>
os: OSConfig,
jvm: JVMConfig,
```

**Datastream:** `settings-logstash.node-esdiag`

There is one `node` doc and fields nest under `node`.
+ Includes `build_*` properties from `logstash_version.*`
+ Includes `plugin.count` from `logstash_plugins.total`
+ Calculates `pipeline.count`
- Extracts `pipelines`

**Datastream:** `settings-logstash.pipeline-esdiag`

Each `pipelines` entry is a doc with fields nested under `pipeline`.

####  logstash_node_stats.json

```yaml
jvm: JVMStats,
process: ProcessStats,
events: EventStats,
flow: FlowStats,
pipelines: HashMap<PipelineStats>,
reloads: ReloadStats,
os: OSStats,
queue: QueueStats,
```

**Datastream:** `metrics-logstash.node-esdiag`

There is one `node` doc and fields nest under `node`.
- Extracts `pipelines`

**Datastream:** `metrics-logstash.pipeline-esdiag`

Each `pipelines` entry is a doc with fields nested under `pipeline`.
- Extracts `plugins`

**Datastream:** `metrics-logstash.plugin-esdiag`

Each `plugins` entry is a doc with fields nested under `plugin`.

####  logstash_nodes_hot_threads.json

**Datastream:** `logs-logstash.hot_threads-esdiag`

```yaml
hot_threads: HotThreads
```

Each `hot_threads` entry is a doc, with fields nested under `hot_thread`.

####  logstash_plugins.json

**Datastream:** `settings-logstash.plugins-esdiag`

```yaml
total: Number
plugins: PluginList
```

Each `plugins` entry is a doc, with fields nested under `plugin`.
