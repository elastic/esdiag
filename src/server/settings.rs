use super::ServerState;
use crate::server::template::SettingsModal;
use crate::data::{KnownHost, KnownHostBuilder, Settings, Uri};
use crate::exporter::Exporter;
use askama::Template;
use async_stream::stream;
use axum::{
    extract::State,
    response::{IntoResponse, Sse},
};
use datastar::prelude::{ElementPatchMode, PatchElements};
use serde::Deserialize;
use std::sync::Arc;

pub async fn get_modal(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    let settings = Settings::load().unwrap_or_default();
    
    // Get list of known hosts from file
    let hosts = KnownHost::list_all().unwrap_or_default();
    
    let active_target = settings.active_target.clone().unwrap_or_default();
    let kibana_url = state.kibana_url.read().await.clone();

    let modal = SettingsModal {
        hosts,
        active_target,
        kibana_url,
    };

    Sse::new(stream! {
        match modal.render() {
            Ok(html) => yield Ok::<_, std::convert::Infallible>(PatchElements::new(html).mode(ElementPatchMode::Append).selector("body").write_as_axum_sse_event()),
            Err(err) => yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div>Error: {}</div>", err)).write_as_axum_sse_event()),
        }
    })
}

#[derive(Deserialize, Default)]
pub struct UpdateSettingsForm {
    target: String,
    new_host_name: Option<String>,
    new_host_url: Option<String>,
    new_host_apikey: Option<String>,
    new_host_username: Option<String>,
    new_host_password: Option<String>,
    kibana_url: Option<String>,
}

pub async fn update_settings(
    State(state): State<Arc<ServerState>>,
    datastar::axum::ReadSignals(signals): datastar::axum::ReadSignals<super::Signals>,
) -> impl IntoResponse {
    let mut settings = Settings::load().unwrap_or_default();
    let form = signals.settings;

    // 1. Process target selection
    if form.target == "new_host" {
        // Build new host
        if let (Some(name), Some(url)) = (&form.new_host_name, &form.new_host_url) {
            if !name.is_empty() && !url.is_empty() {
                match url.parse() {
                    Ok(parsed_url) => {
                        let mut builder = KnownHostBuilder::new(parsed_url);
                        
                        if let Some(apikey) = &form.new_host_apikey {
                            if !apikey.is_empty() {
                                builder = builder.apikey(Some(apikey.clone()));
                            }
                        } else if let (Some(user), Some(pass)) = (&form.new_host_username, &form.new_host_password) {
                            if !user.is_empty() && !pass.is_empty() {
                                builder = builder.username(Some(user.clone())).password(Some(pass.clone()));
                            }
                        }
                        
                        match builder.build() {
                            Ok(host) => {
                                // Validate connection before saving
                                let is_valid = match Uri::try_from(host.clone()) {
                                    Ok(uri) => {
                                        match crate::client::Client::try_from(uri) {
                                            Ok(client) => {
                                                match client.test_connection().await {
                                                    Ok(_) => true,
                                                    Err(e) => {
                                                        let err_msg = format!("Failed to connect to new host: {}", e);
                                                        log::error!("{}", err_msg);
                                                        return Sse::new(stream! {
                                                            yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                                                        });
                                                    }
                                                }
                                            },
                                            Err(e) => {
                                                let err_msg = format!("Failed to construct client: {}", e);
                                                log::error!("{}", err_msg);
                                                return Sse::new(stream! {
                                                    yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                                                });
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        let err_msg = format!("Failed to parse host into URI: {}", e);
                                        log::error!("{}", err_msg);
                                        return Sse::new(stream! {
                                            yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                                        });
                                    }
                                };
                                
                                if is_valid {
                                    match host.save(name) {
                                        Ok(_) => {
                                            settings.active_target = Some(name.clone());
                                        }
                                        Err(e) => {
                                            let err_msg = format!("Failed to save host to hosts.yml: {}", e);
                                            log::error!("{}", err_msg);
                                            return Sse::new(stream! {
                                                yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                                            });
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let err_msg = format!("Failed to configure host: {}", e);
                                log::error!("{}", err_msg);
                                return Sse::new(stream! {
                                    yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("Invalid URL provided for new host: {}", e);
                        log::error!("{}", err_msg);
                        return Sse::new(stream! {
                            yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                        });
                    }
                }
            }
        }
    } else {
        settings.active_target = Some(form.target.clone());
    }

    // 2. Process kibana URL
    if let Some(kibana) = form.kibana_url {
        settings.kibana_url = Some(kibana.clone());
        *state.kibana_url.write().await = kibana;
    }

    // 3. Save settings to disk
    if let Err(e) = settings.save() {
        let err_msg = format!("Failed to save settings: {}", e);
        log::error!("{}", err_msg);
        return Sse::new(stream! {
            yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
        });
    }

    // 4. Update the active Exporter in ServerState
    if let Some(target) = &settings.active_target {
        match KnownHost::get_known(target).ok_or_else(|| eyre::eyre!("Host not found")) {
            Ok(host) => {
                match Uri::try_from(host) {
                    Ok(uri) => {
                        match Exporter::try_from(uri) {
                            Ok(new_exporter) => {
                                *state.exporter.write().await = new_exporter;
                            }
                            Err(e) => {
                                let err_msg = format!("Failed to construct exporter: {}", e);
                                log::error!("{}", err_msg);
                                return Sse::new(stream! {
                                    yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("Invalid Host URI: {}", e);
                        log::error!("{}", err_msg);
                        return Sse::new(stream! {
                            yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                        });
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Could not find Target in hosts.yml: {}", e);
                log::error!("{}", err_msg);
                return Sse::new(stream! {
                    yield Ok::<_, std::convert::Infallible>(PatchElements::new(format!("<div id='settings-error' style='color: red; padding: 10px;'>{}</div>", err_msg)).mode(ElementPatchMode::Prepend).selector("#settings-form").write_as_axum_sse_event());
                });
            }
        }
    }
    
    // 5. Build response to remove modal and update exporter text
    // Return HTML block to close the modal using JavaScript, or we can use Datastar events here
    // but the easiest is simple HTML since we just need to execute it or trigger a reload
    Sse::new(stream! {
        yield Ok::<_, std::convert::Infallible>(PatchElements::new(r#"
        <div id="settings-modal" data-on-load="window.location.reload();">
            Reloading...
        </div>
        "#).write_as_axum_sse_event());
    })
}
