use super::ServerState;
use crate::server::template::SettingsModal;
use crate::data::{KnownHost, KnownHostBuilder, Settings, Uri};
use crate::exporter::Exporter;
use askama::Template;
use async_stream::stream;
use axum::{
    extract::State,
    response::{IntoResponse, Sse},
    Form,
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

#[derive(Deserialize)]
pub struct UpdateSettingsForm {
    target: String,
    new_host_name: Option<String>,
    new_host_url: Option<String>,
    new_host_apikey: Option<String>,
    kibana_url: Option<String>,
}

pub async fn update_settings(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<UpdateSettingsForm>,
) -> impl IntoResponse {
    let mut settings = Settings::load().unwrap_or_default();

    // 1. Process target selection
    if form.target == "new_host" {
        // Build new host
        if let (Some(name), Some(url), Some(apikey)) = (form.new_host_name, form.new_host_url, form.new_host_apikey) {
            if !name.is_empty() && !url.is_empty() {
                let builder = KnownHostBuilder::new(url.parse().unwrap());
                if let Ok(host) = builder.apikey(Some(apikey)).build() {
                    let _ = host.save(&name);
                    settings.active_target = Some(name);
                }
            }
        }
    } else {
        settings.active_target = Some(form.target);
    }

    // 2. Process kibana URL
    if let Some(kibana) = form.kibana_url {
        settings.kibana_url = Some(kibana.clone());
        *state.kibana_url.write().await = kibana;
    }

    // 3. Save settings to disk
    let _ = settings.save();

    // 4. Update the active Exporter in ServerState
    if let Some(target) = &settings.active_target {
        if let Ok(host) = KnownHost::get_known(target).ok_or_else(|| eyre::eyre!("Host not found")) {
            if let Ok(uri) = Uri::try_from(host) {
                if let Ok(new_exporter) = Exporter::try_from(uri) {
                    *state.exporter.write().await = new_exporter;
                }
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
