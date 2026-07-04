// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::{Auth, KnownHost};
use eyre::{Context, Result};
use reqwest::Method;
use std::collections::HashMap;
use url::Url;

pub(crate) const KIBANA_REQUEST_CONCURRENCY: usize = 5;

/// An exporter that sends requests to an Kibana cluster.
#[derive(Clone, Debug)]
pub struct KibanaClient {
    inner: kibana_sync::KibanaClient,
}

/// A reqwest-based client with authentication for Kibana
impl KibanaClient {
    /// Create a new KibanaExporter from a URL and Auth
    pub fn try_new(url: Url, auth: Auth) -> Result<Self> {
        Self::try_new_with_concurrency(url, auth, KIBANA_REQUEST_CONCURRENCY)
    }

    pub(crate) fn try_new_with_concurrency(url: Url, auth: Auth, max_concurrency: usize) -> Result<Self> {
        let inner = kibana_sync::KibanaClient::builder(url)
            .auth(to_kibana_sync_auth(auth))
            .max_concurrency(max_concurrency)
            .build()
            .wrap_err("Failed to build Kibana client")?;

        Ok(Self { inner })
    }

    /// Send a request to a given path on the Kibana client
    pub async fn request(
        &self,
        method: Method,
        headers: &HashMap<String, String>,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<reqwest::Response> {
        self.inner
            .request(method, headers, path, body)
            .await
            .wrap_err("Failed to send request")
    }

    /// Verify the connection and authentication to Kibana
    pub async fn test_connection(&self) -> Result<reqwest::Response> {
        self.request(Method::GET, &HashMap::new(), "/api/status", None).await
    }

    #[cfg(test)]
    fn inner(&self) -> &kibana_sync::KibanaClient {
        &self.inner
    }
}

impl TryFrom<KnownHost> for KibanaClient {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url()?;
        let auth = host.get_auth()?;
        KibanaClient::try_new(url, auth)
    }
}

impl std::fmt::Display for KibanaClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

fn to_kibana_sync_auth(auth: Auth) -> kibana_sync::Auth {
    match auth {
        Auth::Apikey(apikey) => kibana_sync::Auth::Apikey(apikey),
        Auth::Basic(username, password) => kibana_sync::Auth::Basic(username, password),
        Auth::None => kibana_sync::Auth::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HostRole, Product};
    use futures::future::join_all;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::Mutex,
        time::{Duration, sleep},
    };

    #[test]
    fn auth_mapping_preserves_basic_api_key_and_none_modes() {
        assert!(matches!(
            to_kibana_sync_auth(Auth::Basic("elastic".to_string(), "secret".to_string())),
            kibana_sync::Auth::Basic(username, password) if username == "elastic" && password == "secret"
        ));
        assert!(matches!(
            to_kibana_sync_auth(Auth::Apikey("encoded".to_string())),
            kibana_sync::Auth::Apikey(key) if key == "encoded"
        ));
        assert!(matches!(to_kibana_sync_auth(Auth::None), kibana_sync::Auth::None));
    }

    #[test]
    fn known_host_conversion_builds_shared_client_with_display_url() {
        let host = KnownHost::new_no_auth(
            Product::Kibana,
            Url::parse("http://localhost:5601").expect("url"),
            vec![HostRole::Collect],
            None,
            false,
        );

        let client = KibanaClient::try_from(host).expect("client");

        assert_eq!(client.to_string(), "http://localhost:5601/");
        assert_eq!(client.inner().url().as_str(), "http://localhost:5601/");
    }

    #[tokio::test]
    async fn request_headers_map_basic_api_key_and_none_auth() {
        let basic = capture_single_request(|url| async move {
            let client =
                KibanaClient::try_new(url, Auth::Basic("elastic".to_string(), "changeme".to_string())).expect("client");
            let _ = client.test_connection().await.expect("response");
        })
        .await;
        assert!(
            basic.contains("authorization: Basic ZWxhc3RpYzpjaGFuZ2VtZQ=="),
            "unexpected request:\n{basic}"
        );
        assert!(basic.contains("kbn-xsrf: true"), "unexpected request:\n{basic}");

        let api_key = capture_single_request(|url| async move {
            let client = KibanaClient::try_new(url, Auth::Apikey("key-material".to_string())).expect("client");
            let _ = client.test_connection().await.expect("response");
        })
        .await;
        assert!(
            api_key.contains("authorization: ApiKey key-material"),
            "unexpected request:\n{api_key}"
        );

        let no_auth = capture_single_request(|url| async move {
            let client = KibanaClient::try_new(url, Auth::None).expect("client");
            let _ = client.test_connection().await.expect("response");
        })
        .await;
        assert!(
            !no_auth.to_ascii_lowercase().contains("authorization:"),
            "no-auth requests must omit Authorization header:\n{no_auth}"
        );
    }

    #[tokio::test]
    async fn client_concurrency_limit_is_enforced() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let url = Url::parse(&format!("http://{}", listener.local_addr().expect("addr"))).expect("url");
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let active_server = active.clone();
        let max_server = max_active.clone();

        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (stream, _) = listener.accept().await.expect("accept");
                let active = active_server.clone();
                let max_active = max_server.clone();
                tokio::spawn(async move {
                    active.fetch_add(1, Ordering::SeqCst);
                    let current = active.load(Ordering::SeqCst);
                    max_active.fetch_max(current, Ordering::SeqCst);
                    sleep(Duration::from_millis(40)).await;
                    write_ok(stream).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        let client = KibanaClient::try_new_with_concurrency(url, Auth::None, 1).expect("client");
        let requests = (0..3).map(|_| client.test_connection());
        let responses = join_all(requests).await;
        for response in responses {
            assert!(response.expect("response").status().is_success());
        }
        server.await.expect("server");

        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn space_prefixed_paths_are_not_double_prefixed_by_root_client() {
        let request = capture_single_request(|url| async move {
            let client = KibanaClient::try_new(url, Auth::None).expect("client");
            let _ = client
                .request(
                    Method::GET,
                    &HashMap::new(),
                    "/s/marketing/api/saved_objects/_find",
                    None,
                )
                .await
                .expect("response");
        })
        .await;

        assert!(request.starts_with("GET /s/marketing/api/saved_objects/_find HTTP/1.1"));
        assert!(!request.contains("/s/marketing/s/marketing/"));
    }

    #[tokio::test]
    async fn multipart_request_uses_shared_client_form_upload_shape() {
        let request = capture_single_request(|url| async move {
            let client = KibanaClient::try_new(url, Auth::None).expect("client");
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "multipart/form-data".to_string());
            let _ = client
                .request(
                    Method::POST,
                    &headers,
                    "/api/saved_objects/_import",
                    Some(b"{\"type\":\"dashboard\"}\n"),
                )
                .await
                .expect("response");
        })
        .await;

        assert!(
            request.contains("content-type: multipart/form-data; boundary="),
            "unexpected request:\n{request}"
        );
        assert!(
            request.contains("name=\"file\"") && request.contains("filename=\"dashboards.ndjson\""),
            "unexpected request:\n{request}"
        );
        assert!(request.contains("Content-Type: application/x-ndjson"));
        assert!(request.contains("{\"type\":\"dashboard\"}"));
    }

    async fn capture_single_request<F, Fut>(run: F) -> String
    where
        F: FnOnce(Url) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let url = Url::parse(&format!("http://{}", listener.local_addr().expect("addr"))).expect("url");
        let captured = Arc::new(Mutex::new(String::new()));
        let captured_server = captured.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let request = read_http_request(&mut stream).await;
            *captured_server.lock().await = request;
            write_ok(stream).await;
        });

        run(url).await;
        server.await.expect("server");
        captured.lock().await.clone()
    }

    async fn read_http_request(stream: &mut TcpStream) -> String {
        const MAX_TEST_REQUEST_BYTES: usize = 64 * 1024;

        let mut request = Vec::new();
        let mut buf = [0_u8; 1024];
        loop {
            let read = stream.read(&mut buf).await.expect("read request");
            assert_ne!(read, 0, "connection closed before request completed");
            request.extend_from_slice(&buf[..read]);
            assert!(
                request.len() <= MAX_TEST_REQUEST_BYTES,
                "request exceeded test helper limit"
            );

            let request_text = String::from_utf8_lossy(&request);
            let Some(header_end) = request_text.find("\r\n\r\n") else {
                continue;
            };
            let content_length = request_text[..header_end]
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            let expected = header_end + 4 + content_length;
            assert!(
                expected <= MAX_TEST_REQUEST_BYTES,
                "request content exceeded test helper limit"
            );
            if request.len() >= expected {
                return String::from_utf8_lossy(&request[..expected]).to_string();
            }
        }
    }

    async fn write_ok(mut stream: TcpStream) {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 42\r\ncontent-type: application/json\r\n\r\n{\"name\":\"test-kibana\",\"version\":{\"number\":\"9.0.0\"}}")
            .await
            .expect("write response");
    }
}
