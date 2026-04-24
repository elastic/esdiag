#![cfg(feature = "server")]

use esdiag::{
    exporter::Exporter,
    server::{RuntimeMode, Server},
};
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

async fn start_server() -> (Server, Client, String) {
    let (server, bound_addr) = Server::start([127, 0, 0, 1], 0, Exporter::default(), String::new(), RuntimeMode::User)
        .await
        .expect("start local server");
    let client = Client::new();
    let base = format!("http://127.0.0.1:{}", bound_addr.port());

    for _ in 0..40 {
        if client.get(format!("{base}/")).send().await.is_ok() {
            return (server, client, base);
        }
        sleep(Duration::from_millis(25)).await;
    }

    panic!("server did not become reachable in time");
}

#[tokio::test]
async fn test_client_hint_headers_and_theme_resolution() {
    let (mut server, client, base) = start_server().await;

    let response = client
        .get(format!("{base}/"))
        .header("Sec-CH-Prefers-Color-Scheme", "dark")
        .send()
        .await
        .expect("index response");
    assert!(response.status().is_success());

    let accept_ch = response
        .headers()
        .get("accept-ch")
        .expect("accept-ch header")
        .to_str()
        .expect("valid accept-ch");
    assert!(accept_ch.contains("Sec-CH-Prefers-Color-Scheme"));

    let critical_ch = response
        .headers()
        .get("critical-ch")
        .expect("critical-ch header")
        .to_str()
        .expect("valid critical-ch");
    assert!(critical_ch.contains("Sec-CH-Prefers-Color-Scheme"));

    let vary_values = response
        .headers()
        .get_all("vary")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect::<Vec<_>>()
        .join(",");
    assert!(vary_values.contains("Sec-CH-Prefers-Color-Scheme"));
    assert!(vary_values.contains("Cookie"));

    let body = response.text().await.expect("response body");
    assert!(body.contains("id=\"dark-mode\" type=\"checkbox\" data-bind:theme.dark checked"));

    let cookie_override = client
        .get(format!("{base}/"))
        .header("Sec-CH-Prefers-Color-Scheme", "dark")
        .header("Cookie", "theme_dark=0")
        .send()
        .await
        .expect("index response with cookie");
    assert!(cookie_override.status().is_success());
    let cookie_override_body = cookie_override.text().await.expect("cookie body");
    assert!(!cookie_override_body.contains("id=\"dark-mode\" type=\"checkbox\" data-bind:theme.dark checked"));

    server.shutdown().await;
}

#[tokio::test]
async fn test_docs_routes_render_and_404() {
    let (mut server, client, base) = start_server().await;

    let docs_index = client
        .get(format!("{base}/docs/documentation"))
        .send()
        .await
        .expect("docs index response");
    assert!(docs_index.status().is_success());
    let docs_index_body = docs_index.text().await.expect("docs index body");
    assert!(docs_index_body.contains("ESDiag Documentation"));
    assert!(docs_index_body.contains("href=\"#bin/esdiag-control\""));

    let subtopic = client
        .get(format!("{base}/docs/bin/esdiag-control"))
        .send()
        .await
        .expect("subtopic response");
    assert!(subtopic.status().is_success());
    let subtopic_body = subtopic.text().await.expect("subtopic body");
    assert!(subtopic_body.contains("Elastic Stack Diagnostics Control"));
    assert!(subtopic_body.contains("esdiag-control buildx --push"));

    let missing = client
        .get(format!("{base}/docs/no/such/page"))
        .send()
        .await
        .expect("missing response");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
    let missing_body = missing.text().await.expect("missing body");
    assert!(missing_body.contains("Document not found"));

    server.shutdown().await;
}

#[tokio::test]
async fn index_embeds_processing_option_catalog() {
    let (mut server, client, base) = start_server().await;

    let response = client
        .get(format!("{base}/workflow"))
        .send()
        .await
        .expect("workflow response");
    assert!(response.status().is_success());
    let body = response.text().await.expect("workflow body");

    assert!(body.contains("const PROCESS_OPTIONS ="));
    assert!(body.contains("\"elasticsearch\""));
    assert!(body.contains("\"cluster_settings_defaults\""));
    assert!(body.contains("\"logstash\""));
    assert!(body.contains("\"plugins\""));

    server.shutdown().await;
}
