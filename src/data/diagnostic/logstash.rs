use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum DataSet {
    HotThreads,
    Node,
    NodeStats,
    Plugins,
    Version,
}

impl std::fmt::Display for DataSet {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSet::HotThreads => write!(fmt, "logstash_hot_threads"),
            DataSet::Node => write!(fmt, "logstash_node"),
            DataSet::NodeStats => write!(fmt, "logstash_node_stats"),
            DataSet::Plugins => write!(fmt, "logstash_plugins"),
            DataSet::Version => write!(fmt, "logstash_version"),
        }
    }
}
