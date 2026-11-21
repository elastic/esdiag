//! Opt-in regression test for a disposable, unauthenticated Elasticsearch cluster.
//! Installs repository assets and rolls over test streams. Never run on a shared cluster.

use reqwest::{Method, blocking::Client};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

struct Cluster {
    client: Client,
    url: String,
}

impl Cluster {
    fn request(&self, method: Method, path: &str, body: Option<Value>) -> Value {
        let mut request = self.client.request(method, format!("{}{path}", self.url));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().expect("Elasticsearch request");
        let status = response.status();
        let result: Value = response.json().expect("Elasticsearch JSON response");
        assert!(status.is_success(), "{path}: {status} {result}");
        result
    }
}

#[test]
#[ignore = "requires ESDIAG_TEST_ES_URL pointing to a disposable Elasticsearch cluster"]
fn mixed_version_provenance_writes_survive_rollover() {
    let cluster = Cluster {
        client: Client::builder().timeout(Duration::from_secs(60)).build().unwrap(),
        url: std::env::var("ESDIAG_TEST_ES_URL")
            .expect("set ESDIAG_TEST_ES_URL to a disposable cluster")
            .trim_end_matches('/')
            .to_owned(),
    };
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/elasticsearch");
    let run = uuid::Uuid::new_v4().to_string();
    for (folder, endpoint, suffix) in [
        ("ingest_pipelines", "_ingest/pipeline", ""),
        ("component_templates", "_component_template", ""),
        ("index_templates", "_index_template", "-esdiag"),
    ] {
        let mut paths: Vec<_> = std::fs::read_dir(assets.join(folder))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
            .collect();
        paths.sort();
        for path in paths {
            let name = path.file_stem().unwrap().to_str().unwrap();
            let body = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            cluster.request(Method::PUT, &format!("/{endpoint}/{name}{suffix}"), Some(body));
        }
    }
    for (stream, application) in [
        ("metrics-diagnostic-esdiag", "elasticsearch"),
        ("metrics-node-esdiag", "elasticsearch"),
        ("metrics-logstash.node-esdiag", "logstash"),
    ] {
        let response = cluster
            .client
            .get(format!("{}/_data_stream/{stream}", cluster.url))
            .send()
            .unwrap();
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            cluster.request(Method::PUT, &format!("/_data_stream/{stream}"), None);
        } else {
            assert!(
                response.status().is_success(),
                "reading {stream}: {}",
                response.status()
            );
            cluster.request(Method::POST, &format!("/{stream}/_rollover"), Some(json!({})));
        }
        for generation in 0..2 {
            if generation > 0 {
                cluster.request(Method::POST, &format!("/{stream}/_rollover"), Some(json!({})));
            }
            let mut bulk = String::new();
            for mut diagnostic in [
                json!({"product": application, "orchestration": "eck"}),
                json!({"application": application, "platform": "eck"}),
                json!({"product": application, "application": application, "orchestration": "eck", "platform": "eck"}),
            ] {
                diagnostic["id"] = json!(run);
                let document = json!({"@timestamp": chrono::Utc::now().to_rfc3339(), "diagnostic": diagnostic});
                cluster.request(Method::POST, &format!("/{stream}/_doc"), Some(document.clone()));
                bulk.push_str(&json!({"create": {"_index": stream}}).to_string());
                bulk.push('\n');
                bulk.push_str(&document.to_string());
                bulk.push('\n');
            }
            let response = cluster
                .client
                .post(format!("{}/_bulk", cluster.url))
                .header("Content-Type", "application/x-ndjson")
                .body(bulk)
                .send()
                .unwrap();
            let status = response.status();
            let result: Value = response.json().unwrap();
            assert!(status.is_success(), "bulk: {status} {result}");
            assert_eq!(result["errors"], false, "{result}");
            cluster.request(Method::POST, &format!("/{stream}/_refresh"), None);
            for (field, value) in [
                ("product", application),
                ("application", application),
                ("orchestration", "eck"),
                ("platform", "eck"),
            ] {
                let field = format!("diagnostic.{field}");
                let result = cluster.request(
                    Method::POST,
                    &format!("/{stream}/_search"),
                    Some(json!({
                        "size": 0,
                        "query": {"bool": {"filter": [
                            {"term": {"diagnostic.id": run}}, {"term": {&field: value}}
                        ]}},
                        "aggs": {"values": {"terms": {"field": field}}}
                    })),
                );
                let count = 6 * (generation + 1);
                assert_eq!(result["hits"]["total"]["value"], count, "{stream} {field}: {result}");
                assert_eq!(
                    result["aggregations"]["values"]["buckets"],
                    json!([{"key": value, "doc_count": count}]),
                    "{stream} {field}: {result}"
                );
            }
        }
        println!("PASS {stream}: legacy/current/dual writes, bulk, search, aggregations, rollover");
    }
}
