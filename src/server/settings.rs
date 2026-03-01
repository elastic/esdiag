use super::{ServerState, template::SettingsModal};
use crate::data::{KnownHost, KnownHostBuilder, Settings, Uri};
use crate::exporter::Exporter;
use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse},
    Form,
};
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

    match modal.render() {
        Ok(html) => Html(html),
        Err(err) => Html(format!("<div>Error rendering template: {}</div>", err)),
    }
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
    // Ideally we would send back multiple events. For now we will return standard HTML to close it and force a hard refresh
    // Or we can just use Datastar signals to trigger a reload.
    // Easiest is to close the modal and update the UI with a reload.
    Html(r#"
        <script>
            document.getElementById('settings-modal').remove();
            window.location.reload();
        </script>
    "#.to_string())
}
