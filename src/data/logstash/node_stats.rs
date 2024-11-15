use crate::data::{
    diagnostic::{data_source::DataSource, logstash::DataSet},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
pub struct LogstashNodeStats {
    host: String,
    version: String,
    http_address: String,
    id: String,
    name: String,
    ephemeral_id: String,
    status: String,
    snapshot: bool,
    pipeline: PipelineStats,
    jvm: JvmStats,
    process: ProcessStats,
    events: EventStats,
    flow: FlowStats,
    pipelines: HashMap<String, PipelineDetails>,
    reloads: ReloadStats,
    os: OsStats,
    queue: QueueStats,
}

#[derive(Deserialize, Serialize)]
struct PipelineStats {
    workers: u32,
    batch_size: u32,
    batch_delay: u32,
}

#[derive(Deserialize, Serialize)]
struct JvmStats {
    threads: ThreadStats,
    mem: MemoryStats,
    gc: GarbageCollectionStats,
    uptime_in_millis: usize,
}

#[derive(Deserialize, Serialize)]
struct ThreadStats {
    count: u32,
    peak_count: u32,
}

#[derive(Deserialize, Serialize)]
struct MemoryStats {
    heap_used_percent: u32,
    heap_committed_in_bytes: usize,
    heap_max_in_bytes: usize,
    heap_used_in_bytes: usize,
    non_heap_used_in_bytes: usize,
    non_heap_committed_in_bytes: usize,
    pools: PoolsStats,
}

#[derive(Deserialize, Serialize)]
struct PoolsStats {
    survivor: PoolStats,
    young: PoolStats,
    old: PoolStats,
}

#[derive(Deserialize, Serialize)]
struct PoolStats {
    peak_used_in_bytes: usize,
    committed_in_bytes: usize,
    used_in_bytes: usize,
    peak_max_in_bytes: isize,
    max_in_bytes: isize,
}

#[derive(Deserialize, Serialize)]
struct GarbageCollectionStats {
    collectors: HashMap<String, CollectorStats>,
}

#[derive(Deserialize, Serialize)]
struct CollectorStats {
    collection_time_in_millis: usize,
    collection_count: u32,
}

#[derive(Deserialize, Serialize)]
struct ProcessStats {
    open_file_descriptors: u32,
    peak_open_file_descriptors: u32,
    max_file_descriptors: u32,
    mem: ProcessMemoryStats,
    cpu: ProcessCpuStats,
}

#[derive(Deserialize, Serialize)]
struct ProcessMemoryStats {
    total_virtual_in_bytes: usize,
}

#[derive(Deserialize, Serialize)]
struct ProcessCpuStats {
    total_in_millis: usize,
    percent: u32,
    load_average: LoadAverageStats,
}

#[derive(Deserialize, Serialize)]
struct LoadAverageStats {
    #[serde(rename = "1m")]
    one: OptionF64,
    #[serde(rename = "5m")]
    five: OptionF64,
    #[serde(rename = "15m")]
    fifteen: OptionF64,
}

#[derive(Deserialize, Serialize)]
struct EventStats {
    r#in: usize,
    filtered: usize,
    out: usize,
    duration_in_millis: usize,
    queue_push_duration_in_millis: usize,
}

#[derive(Deserialize, Serialize)]
struct FlowStats {
    input_throughput: TrailingStats,
    filter_throughput: TrailingStats,
    output_throughput: TrailingStats,
    queue_backpressure: TrailingStats,
    worker_concurrency: TrailingStats,
}

#[derive(Deserialize, Serialize)]
struct PipelineDetails {
    r#events: PipelineEventsStats,
    flow: PipelineFlowStats,
    plugins: PipelinePlugins,
    reloads: PipelineReloadStats,
    queue: PipelineQueueStats,
    hash: String,
    ephemeral_id: String,
}

#[derive(Deserialize, Serialize)]
struct PipelineEventsStats {
    out: usize,
    duration_in_millis: usize,
    filtered: usize,
    r#in: usize,
    queue_push_duration_in_millis: usize,
}

#[derive(Deserialize, Serialize)]
struct PipelineFlowStats {
    queue_backpressure: TrailingStats,
    output_throughput: TrailingStats,
    input_throughput: TrailingStats,
    queue_persisted_growth_bytes: Option<TrailingStats>,
    filter_throughput: TrailingStats,
    worker_concurrency: TrailingStats,
    queue_persisted_growth_events: Option<TrailingStats>,
}

#[derive(Deserialize, Serialize)]
struct PipelinePlugins {
    inputs: Vec<PluginDetails>,
    codecs: Vec<CodecDetails>,
    filters: Vec<FilterDetails>,
    outputs: Vec<OutputDetails>,
}

#[derive(Deserialize, Serialize)]
struct PluginDetails {
    id: String,
    flow: PluginFlowStats,
    name: String,
    events: PluginEventsStats,
    address: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct PluginFlowStats {
    throughput: TrailingStats,
}

#[derive(Deserialize, Serialize)]
struct PluginEventsStats {
    out: usize,
    queue_push_duration_in_millis: usize,
}

#[derive(Deserialize, Serialize)]
struct CodecDetails {
    id: String,
    encode: CodecStats,
    name: String,
    decode: CodecStats,
}

#[derive(Deserialize, Serialize)]
struct CodecStats {
    writes_in: usize,
    duration_in_millis: usize,
}

#[derive(Deserialize, Serialize)]
struct FilterDetails {
    id: String,
    flow: FilterFlowStats,
    name: String,
    r#events: FilterEventsStats,
}

#[derive(Deserialize, Serialize)]
struct FilterFlowStats {
    worker_millis_per_event: TrailingStats,
    worker_utilization: TrailingStats,
}

#[derive(Deserialize, Serialize)]
struct TrailingStats {
    current: OptionF64,
    last_1_minute: OptionF64,
    last_5_minutes: OptionF64,
    last_15_minutes: OptionF64,
    last_1_hour: OptionF64,
    lifetime: OptionF64,
}

#[derive(Deserialize, Serialize)]
struct FilterEventsStats {
    out: usize,
    duration_in_millis: usize,
    r#in: usize,
}

#[derive(Deserialize, Serialize)]
struct OutputDetails {
    id: String,
    flow: OutputFlowStats,
    name: String,
    r#events: OutputEventsStats,
    documents: Option<OutputDocumentsStats>,
    bulk_requests: Option<OutputBulkRequestsStats>,
}

#[derive(Deserialize, Serialize)]
struct OutputFlowStats {
    worker_millis_per_event: TrailingStats,
    worker_utilization: TrailingStats,
}

#[derive(Deserialize, Serialize)]
struct OutputEventsStats {
    out: usize,
    duration_in_millis: usize,
    r#in: usize,
}

#[derive(Deserialize, Serialize)]
struct OutputDocumentsStats {
    non_retryable_failures: usize,
    successes: usize,
}

#[derive(Deserialize, Serialize)]
struct OutputBulkRequestsStats {
    with_errors: u32,
    responses: HashMap<String, u32>,
    successes: u32,
}

#[derive(Deserialize, Serialize)]
struct PipelineReloadStats {
    failures: u32,
    last_failure_timestamp: Option<String>,
    last_error: Option<String>,
    successes: u32,
    last_success_timestamp: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct PipelineQueueStats {
    r#type: String,
    capacity: Option<QueueCapacityStats>,
    events: Option<usize>,
    data: Option<QueueDataStats>,
    events_count: usize,
    queue_size_in_bytes: usize,
    max_queue_size_in_bytes: usize,
}

#[derive(Deserialize, Serialize)]
struct QueueCapacityStats {
    page_capacity_in_bytes: usize,
    max_queue_size_in_bytes: usize,
    queue_size_in_bytes: usize,
    max_unread_events: u32,
}

#[derive(Deserialize, Serialize)]
struct QueueDataStats {
    storage_type: String,
    free_space_in_bytes: usize,
    path: String,
}

#[derive(Deserialize, Serialize)]
struct ReloadStats {
    failures: u32,
    successes: u32,
}

#[derive(Deserialize, Serialize)]
struct OsStats {
    cgroup: CgroupStats,
}

#[derive(Deserialize, Serialize)]
struct CgroupStats {
    cpu: CpuStats,
    cpuacct: CpuAcctStats,
}

#[derive(Deserialize, Serialize)]
struct CpuStats {
    cfs_period_micros: u32,
    cfs_quota_micros: u32,
    control_group: String,
    stat: CpuStatDetails,
}

#[derive(Deserialize, Serialize)]
struct CpuStatDetails {
    time_throttled_nanos: usize,
    number_of_elapsed_periods: u32,
    number_of_times_throttled: u32,
}

#[derive(Deserialize, Serialize)]
struct CpuAcctStats {
    usage_nanos: usize,
    control_group: String,
}

#[derive(Deserialize, Serialize)]
struct QueueStats {
    events_count: usize,
}

impl DataSource for LogstashNodeStats {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("logstash_node_stats.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_node/stats"),
            _ => Err(eyre!("Unsupported source for Logstash node stats")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::NodeStats)
    }
}

#[derive(Serialize)]
struct OptionF64(Option<f64>);

impl From<String> for OptionF64 {
    fn from(value: String) -> Self {
        match value.parse::<f64>() {
            Ok(v) => OptionF64(Some(v)),
            Err(_) => OptionF64(None),
        }
    }
}

impl From<f64> for OptionF64 {
    fn from(value: f64) -> Self {
        OptionF64(Some(value))
    }
}

impl<'de> Deserialize<'de> for OptionF64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if let Ok(value) = f64::deserialize(deserializer) {
            Ok(OptionF64::from(value))
        } else {
            Ok(OptionF64(None))
        }
    }
}
