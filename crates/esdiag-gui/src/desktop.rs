// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::server::{RuntimeMode, Server};
use esdiag_core::{data::Uri, exporter::Exporter};

/// Runs the desktop app: starts the embedded web server on an ephemeral local
/// port, opens it in the `main` window, and stops the server when all windows
/// close. `context` must come from `tauri::generate_context!()` in the crate
/// whose build script ran `tauri_build`.
pub fn run(context: tauri::Context<tauri::Wry>) {
    // Set up communication channel to tell the server when to shut down
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Tauri desktop wrapper logic
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            use tauri::Manager;

            let handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                let settings = esdiag_core::data::Settings::load().unwrap_or_default();

                let exporter = if let Some(target) = &settings.active_target {
                    if let Ok(host) =
                        esdiag_core::data::KnownHost::get_known(target).ok_or_else(|| eyre::eyre!("Host not found"))
                    {
                        if let Ok(uri) = Uri::try_from(host) {
                            Exporter::try_from(uri).unwrap_or_default()
                        } else {
                            Exporter::default()
                        }
                    } else {
                        Exporter::default()
                    }
                } else {
                    Exporter::default()
                };

                let kibana_url = settings.kibana_url.unwrap_or_else(|| {
                    let url = esdiag_core::env::get_string("ESDIAG_KIBANA_URL")
                        .unwrap_or_else(|_| "http://localhost:5601".to_string());
                    esdiag_core::env::append_kibana_space(&url)
                });

                let (mut server, bound_addr) =
                    match Server::start([127, 0, 0, 1], 0, exporter, kibana_url, RuntimeMode::User).await {
                        Ok(res) => res,
                        Err(e) => {
                            tracing::error!("Failed to start embedded server: {}", e);
                            return;
                        }
                    };

                let url = format!("http://localhost:{}", bound_addr.port());

                if let Ok(url) = tauri::Url::parse(&url) {
                    // Wait a tiny bit to ensure server is ready
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

                    if let Some(window) = handle.get_webview_window("main") {
                        window.on_window_event(|event| {
                            if let tauri::WindowEvent::CloseRequested { .. } = event {
                                // Custom close logic if needed
                            }
                        });
                        let _ = window.navigate(url);
                        let _ = window.set_focus();
                    }
                }

                // Wait for Tauri exit signal
                let _ = shutdown_rx.recv().await;
                server.shutdown().await;
            });

            Ok(())
        })
        .on_window_event(move |_window, event| {
            use tauri::WindowEvent;
            if let WindowEvent::Destroyed = event {
                // All windows closed, signal the server to shut down
                let _ = shutdown_tx.try_send(());
            }
        })
        .run(context)
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[tokio::test]
    async fn embedded_server_starts_and_serves_local_url() {
        let exporter = Exporter::default();
        let kibana_url = String::new();
        let (mut server, bound_addr) = Server::start([127, 0, 0, 1], 0, exporter, kibana_url, RuntimeMode::User)
            .await
            .expect("desktop embedded server should start");

        let url = format!("http://localhost:{}", bound_addr.port());
        let parsed = tauri::Url::parse(&url).expect("desktop URL should be valid");

        let response = reqwest::get(parsed.as_str())
            .await
            .expect("embedded server should accept HTTP requests");
        assert!(
            response.status().is_success(),
            "expected success status from embedded server, got {}",
            response.status()
        );

        server.shutdown().await;
    }

    #[tokio::test]
    async fn embedded_server_avoids_occupied_port_by_using_ephemeral_binding() {
        let occupied_listener = TcpListener::bind("127.0.0.1:0").expect("should reserve a local test port");
        let occupied_port = occupied_listener
            .local_addr()
            .expect("reserved listener has local addr")
            .port();

        let exporter = Exporter::default();
        let kibana_url = String::new();
        let (mut server, bound_addr) = Server::start([127, 0, 0, 1], 0, exporter, kibana_url, RuntimeMode::User)
            .await
            .expect("desktop embedded server should start while another port is occupied");

        assert_ne!(
            bound_addr.port(),
            occupied_port,
            "ephemeral bind should avoid occupied ports"
        );

        let url = format!("http://localhost:{}", bound_addr.port());
        let response = reqwest::get(&url)
            .await
            .expect("embedded server should accept HTTP requests");
        assert!(
            response.status().is_success(),
            "expected success status from embedded server, got {}",
            response.status()
        );

        server.shutdown().await;
        drop(occupied_listener);
    }
}
