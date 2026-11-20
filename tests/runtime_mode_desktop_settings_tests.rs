#![cfg(all(feature = "server", feature = "desktop"))]

use esdiag::{
    exporter::Exporter,
    server::{RuntimeMode, Server},
};
use reqwest::{Client, RequestBuilder, StatusCode};
use std::time::Duration;
use tokio::time::{sleep, timeout};

const SERVICE_IDENTITY: (&str, &str) = ("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com");
/// Stable id of the output-target control in the settings modal.
const TARGET_SELECT_ID: &str = "target-host";

async fn start_server(mode: RuntimeMode) -> (Server, Client, String) {
    let (server, bound_addr) = Server::start([127, 0, 0, 1], 0, Exporter::default(), String::new(), mode)
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

fn with_identity(request: RequestBuilder, identity: Option<(&str, &str)>) -> RequestBuilder {
    match identity {
        Some((name, value)) => request.header(name, value),
        None => request,
    }
}

/// Opening tag (without `<`/`>`) of the element carrying `id="{id}"`, once fully received.
fn element_tag<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let id_at = html.find(&format!("id=\"{id}\""))?;
    let open = html[..id_at].rfind('<')? + 1;
    let close = id_at + html[id_at..].find('>')?;
    Some(&html[open..close])
}

/// Requests the settings modal and returns whether the output-target control in the modal patch
/// pushed over the browser's event stream is disabled.
async fn pushed_target_select_disabled(client: &Client, base: &str, identity: Option<(&str, &str)>) -> bool {
    let mut events = with_identity(client.patch(format!("{base}/events")), identity)
        .send()
        .await
        .expect("open event stream");
    assert_eq!(events.status(), StatusCode::OK);

    let modal = with_identity(client.get(format!("{base}/settings/modal")), identity)
        .send()
        .await
        .expect("settings modal response");
    assert_eq!(modal.status(), StatusCode::NO_CONTENT);

    let mut received = Vec::new();
    let tag = timeout(Duration::from_secs(5), async {
        loop {
            let chunk = events
                .chunk()
                .await
                .expect("read event stream")
                .expect("event stream closed before the settings modal arrived");
            received.extend_from_slice(&chunk);
            let text = String::from_utf8_lossy(&received);
            if let Some(tag) = element_tag(&text, TARGET_SELECT_ID) {
                return tag.to_string();
            }
        }
    })
    .await
    .expect("settings modal was not pushed within five seconds");

    tag.split_whitespace().any(|token| token == "disabled")
}

#[tokio::test]
async fn desktop_settings_modal_service_mode_disables_exporter_changes() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    assert!(
        pushed_target_select_disabled(&client, &base, Some(SERVICE_IDENTITY)).await,
        "service mode must not allow changing the exporter target"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn desktop_settings_modal_user_mode_allows_host_management() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    assert!(
        !pushed_target_select_disabled(&client, &base, None).await,
        "user mode must allow changing the exporter target"
    );

    server.shutdown().await;
}
