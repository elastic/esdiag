cat << 'INNER' >> src/processor/elasticsearch/indices_stats/data.rs
    fn name() -> String {
        "indices_stats".to_string()
    }
}
INNER
cat << 'INNER' >> src/processor/elasticsearch/mapping_stats/data.rs
    fn name() -> String {
        "mapping".to_string()
    }
}
INNER
cat << 'INNER' >> src/processor/elasticsearch/nodes_stats/data.rs
    fn name() -> String {
        "nodes_stats".to_string()
    }
}
INNER
