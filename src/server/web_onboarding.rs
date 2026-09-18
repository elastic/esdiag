use super::{ServerState, get_theme_dark, html_event, signal_event};
use crate::{
    client::Client,
    data::{
        Application, ApplicationConfig, HostRole, KnownHost, OnboardingWorkflow, OutputDeployment, SecretAuth, Uri,
        authenticate, keystore_exists, list_secret_names, load_saved_jobs,
    },
    exporter::Exporter,
    onboarding::{self, CollectHostInput, OutputDeploymentInput},
    server::template::{Welcome, WelcomePage, WelcomeStage},
};
use askama::Template;
use axum::{
    extract::{Form, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use std::{process::Command, sync::Arc, time::Duration};
use url::Url;

#[derive(Deserialize)]
pub(crate) struct IdentityForm {
    user: String,
    workflow: String,
}

#[derive(Deserialize, Default)]
pub(crate) struct KeystoreForm {
    password: String,
    confirm: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OutputForm {
    output_name: String,
    output_url: String,
    viewer_name: String,
    viewer_url: String,
    api_key: String,
    replace: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CollectionForm {
    name: String,
    url: String,
    api_key: String,
    replace: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct DefaultJobForm {
    name: String,
    collect_host: String,
    replace: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct ReplaceForm {
    replace: Option<String>,
}

pub(crate) async fn page(State(state): State<Arc<ServerState>>, headers: HeaderMap) -> Response {
    Html(render_page(&state, &headers, String::new()).await).into_response()
}

pub(crate) async fn service_mode_page() -> impl IntoResponse {
    Html(
        "<!doctype html><title>ESDiag configuration</title><h1>ESDiag configuration is administrator-owned</h1><p>This service is configured by its administrator. Local onboarding is unavailable in service mode.</p>",
    )
}

pub(crate) async fn save_identity(State(state): State<Arc<ServerState>>, Form(form): Form<IdentityForm>) -> Response {
    let result = workflow_from_form(&form.workflow)
        .and_then(|workflow| {
            onboarding::save_user(form.user)?;
            onboarding::save_workflow(workflow)?;
            Ok(())
        })
        .map_err(|err| err.to_string());
    refresh(&state, result.err().unwrap_or_default()).await
}

pub(crate) async fn save_keystore(State(state): State<Arc<ServerState>>, Form(form): Form<KeystoreForm>) -> Response {
    let exists = keystore_exists().unwrap_or(false);
    let migrate_legacy_hosts = !exists && super::keystore::migration_needed();
    let result = if !exists && form.confirm.is_none() {
        Err("Confirm keystore creation to continue.".to_string())
    } else if !exists && form.password.trim().len() < 6 {
        Err("Keystore password must be at least 6 characters.".to_string())
    } else if form.password.trim().is_empty() {
        Err("Keystore password is required.".to_string())
    } else {
        authenticate(form.password.trim())
            .map_err(|err| format!("Failed to unlock the keystore: {err}"))
            .map(|_| form.password.trim().to_string())
    };

    match result {
        Ok(password) => {
            if migrate_legacy_hosts && let Err(err) = KnownHost::migrate_hosts_to_keystore(&password) {
                return refresh(&state, format!("Failed to migrate existing host credentials: {err}")).await;
            }
            state.set_keystore_unlocked(password).await;
            refresh(&state, String::new()).await
        }
        Err(message) => refresh(&state, message).await,
    }
}

pub(crate) async fn save_output(State(state): State<Arc<ServerState>>, Form(form): Form<OutputForm>) -> Response {
    if !workflow_processes_diagnostics() {
        return refresh(
            &state,
            "The selected workflow does not process diagnostics, so it cannot configure an output deployment."
                .to_string(),
        )
        .await;
    }
    let Some(keystore_password) = state.keystore_password().await else {
        return refresh(
            &state,
            "Unlock the keystore before saving output credentials.".to_string(),
        )
        .await;
    };
    let result = save_output_stage(&state, form, &keystore_password).await;
    refresh(&state, result.err().unwrap_or_default()).await
}

pub(crate) async fn install_output_assets(State(state): State<Arc<ServerState>>) -> Response {
    let result = install_output_assets_stage().await;
    refresh(&state, result.err().unwrap_or_default()).await
}

pub(crate) async fn provision_local_output(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<ReplaceForm>,
) -> Response {
    if !workflow_processes_diagnostics() {
        return refresh(
            &state,
            "The selected workflow does not process diagnostics, so it cannot provision a diagnostic cluster."
                .to_string(),
        )
        .await;
    }
    let Some(keystore_password) = state.keystore_password().await else {
        return refresh(
            &state,
            "Unlock the keystore before provisioning a local output.".to_string(),
        )
        .await;
    };
    if onboarding::inspect().is_ok_and(|readiness| readiness.output_configured) && form.replace.is_none() {
        return refresh(
            &state,
            "An output deployment already exists. Confirm replacement to provision the local output.".to_string(),
        )
        .await;
    }

    let result = provision_local_output_stage(&state, &keystore_password).await;
    refresh(&state, result.err().unwrap_or_default()).await
}

pub(crate) async fn save_collection(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<CollectionForm>,
) -> Response {
    let Some(keystore_password) = state.keystore_password().await else {
        return refresh(
            &state,
            "Unlock the keystore before saving collection credentials.".to_string(),
        )
        .await;
    };
    let result = save_collection_stage(form, &keystore_password).await;
    refresh(&state, result.err().unwrap_or_default()).await
}

pub(crate) async fn defer_collection(State(state): State<Arc<ServerState>>) -> Response {
    let result = onboarding::defer_collection().map_err(|err| err.to_string());
    refresh(&state, result.err().unwrap_or_default()).await
}

pub(crate) async fn save_default_job(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<DefaultJobForm>,
) -> Response {
    let result = save_default_job_stage(form);
    refresh(&state, result.err().unwrap_or_default()).await
}

async fn refresh(state: &Arc<ServerState>, message: String) -> Response {
    match render_panel(state, message).await {
        Ok(html) => {
            state.publish_event(html_event(html));
            state.publish_event(signal_event(r#"{"_welcomeProvisioning":false}"#));
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => {
            tracing::error!("Failed to render web onboarding stage: {err}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Clone, Debug)]
struct JourneyModel {
    stage: WelcomeStage,
    user: String,
    workflow_value: String,
    processes_diagnostics: bool,
    show_cluster: bool,
    show_collection: bool,
    show_default_job: bool,
    show_complete: bool,
    collection_deferred: bool,
    keystore_ready: bool,
    keystore_unlocked: bool,
    output_name: String,
    output_url: String,
    viewer_name: String,
    viewer_url: String,
    output_location: String,
    environment_output: bool,
    managed_local_stack: bool,
    container_runtime: String,
    elasticsearch_ready: bool,
    kibana_ready: bool,
    elasticsearch_assets: String,
    kibana_assets: String,
    assets_installed: bool,
    collect_name: String,
    collect_url: String,
    default_job_name: String,
}

#[derive(Clone, Debug, Default)]
struct RuntimeOutputStatus {
    declared: bool,
    configured: bool,
    managed_local: bool,
    elasticsearch_url: String,
    kibana_url: String,
    elasticsearch_ready: bool,
    kibana_ready: bool,
    elasticsearch_assets: crate::setup::AssetStatus,
    kibana_assets: crate::setup::AssetStatus,
}

async fn render_page(state: &Arc<ServerState>, headers: &HeaderMap, message: String) -> String {
    let model = journey_model(state).await;
    let (_, request_user) = state
        .resolve_user_email(headers)
        .unwrap_or((false, super::DEFAULT_OWNER.to_string()));
    let user = if model.user.is_empty() {
        request_user
    } else {
        model.user.clone()
    };
    let user_initial = user.chars().next().unwrap_or('_').to_ascii_uppercase();
    let keystore_state = state.keystore_page_state().await;
    WelcomePage {
        debug: tracing::enabled!(tracing::Level::DEBUG),
        desktop: cfg!(feature = "desktop"),
        kibana_url: state.kibana_url.read().await.clone(),
        stats: state.get_stats_as_signals().await,
        stage: stage_name(model.stage).to_string(),
        user,
        workflow_value: model.workflow_value,
        processes_diagnostics: model.processes_diagnostics,
        show_cluster: model.show_cluster,
        show_collection: model.show_collection,
        show_default_job: model.show_default_job,
        show_complete: model.show_complete,
        collection_deferred: model.collection_deferred,
        keystore_ready: model.keystore_ready,
        keystore_unlocked: model.keystore_unlocked,
        output_name: model.output_name,
        output_url: model.output_url,
        viewer_name: model.viewer_name,
        viewer_url: model.viewer_url,
        output_location: model.output_location,
        environment_output: model.environment_output,
        managed_local_stack: model.managed_local_stack,
        container_runtime: model.container_runtime,
        elasticsearch_ready: model.elasticsearch_ready,
        kibana_ready: model.kibana_ready,
        elasticsearch_assets: model.elasticsearch_assets,
        kibana_assets: model.kibana_assets,
        assets_installed: model.assets_installed,
        collect_name: model.collect_name,
        collect_url: model.collect_url,
        default_job_name: model.default_job_name,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark: get_theme_dark(headers),
        runtime_mode: state.runtime_mode.to_string(),
        show_advanced: state.server_policy.allows_advanced(),
        show_job_builder: state.server_policy.allows_job_builder(),
        can_use_keystore: keystore_state.can_use_keystore,
        keystore_locked: keystore_state.locked,
        keystore_lock_time: keystore_state.lock_time,
        message,
    }
    .render()
    .unwrap_or_else(|err| format!("<h1>Unable to render onboarding</h1><p>{err}</p>"))
}

async fn render_panel(state: &Arc<ServerState>, message: String) -> Result<String, askama::Error> {
    let model = journey_model(state).await;
    Welcome {
        stage: stage_name(model.stage).to_string(),
        user: model.user,
        workflow_value: model.workflow_value,
        processes_diagnostics: model.processes_diagnostics,
        show_cluster: model.show_cluster,
        show_collection: model.show_collection,
        show_default_job: model.show_default_job,
        show_complete: model.show_complete,
        collection_deferred: model.collection_deferred,
        keystore_ready: model.keystore_ready,
        keystore_unlocked: model.keystore_unlocked,
        output_name: model.output_name,
        output_url: model.output_url,
        viewer_name: model.viewer_name,
        viewer_url: model.viewer_url,
        output_location: model.output_location,
        environment_output: model.environment_output,
        managed_local_stack: model.managed_local_stack,
        container_runtime: model.container_runtime,
        elasticsearch_ready: model.elasticsearch_ready,
        kibana_ready: model.kibana_ready,
        elasticsearch_assets: model.elasticsearch_assets,
        kibana_assets: model.kibana_assets,
        assets_installed: model.assets_installed,
        collect_name: model.collect_name,
        collect_url: model.collect_url,
        default_job_name: model.default_job_name,
        message,
    }
    .render()
}

async fn journey_model(state: &Arc<ServerState>) -> JourneyModel {
    let config = ApplicationConfig::load().unwrap_or_else(|err| {
        tracing::warn!("Unable to load application configuration for onboarding: {err}");
        ApplicationConfig::new()
    });
    let readiness = onboarding::inspect().unwrap_or_default();
    let runtime_output = runtime_output_status(readiness.output_configured).await;
    let workflow = config.workflow;
    let processes_diagnostics = workflow.is_some_and(OnboardingWorkflow::processes_diagnostics);
    let collects_diagnostics = workflow.is_some_and(OnboardingWorkflow::collects_diagnostics);
    let keystore_unlocked = state.is_keystore_unlocked().await;
    let stage = if !readiness.user_configured || readiness.workflow.is_none() {
        WelcomeStage::Identity
    } else if processes_diagnostics && !readiness.output_configured {
        WelcomeStage::Output
    } else if collects_diagnostics && !readiness.collect_host_configured && !readiness.collection_deferred {
        WelcomeStage::Collection
    } else if collects_diagnostics && readiness.collect_host_configured && !readiness.default_job_configured {
        WelcomeStage::DefaultJob
    } else {
        WelcomeStage::Complete
    };

    let hosts = KnownHost::parse_hosts_yml().unwrap_or_default();
    let mut output_name = config.output.default.clone().unwrap_or_default();
    let output = hosts.get(&output_name);
    let mut output_url = output
        .and_then(KnownHost::concrete_url)
        .map(ToString::to_string)
        .unwrap_or_default();
    let mut viewer_name = output
        .and_then(KnownHost::viewer)
        .map(ToString::to_string)
        .unwrap_or_default();
    let mut viewer_url = hosts
        .get(&viewer_name)
        .and_then(KnownHost::concrete_url)
        .map(ToString::to_string)
        .unwrap_or_default();
    let output_location = if runtime_output.declared {
        if runtime_output.managed_local {
            "local"
        } else {
            "remote"
        }
    } else if !processes_diagnostics {
        ""
    } else {
        output
            .and_then(KnownHost::concrete_url)
            .and_then(|url| url.host_str())
            .filter(|host| matches!(*host, "localhost" | "127.0.0.1"))
            .map(|_| "local")
            .or_else(|| (!output_name.is_empty()).then_some("remote"))
            .unwrap_or_default()
    }
    .to_string();
    if runtime_output.declared {
        output_name = "Environment output".to_string();
        output_url = runtime_output.elasticsearch_url.clone();
        viewer_name = "Environment viewer".to_string();
        viewer_url = runtime_output.kibana_url.clone();
    }
    let (collect_name, collect_url) = hosts
        .iter()
        .find(|(_, host)| host.has_role(HostRole::Collect))
        .map(|(name, host)| {
            (
                name.clone(),
                host.concrete_url().map(ToString::to_string).unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    let show_cluster = !matches!(stage, WelcomeStage::Identity);
    let show_collection = collects_diagnostics
        && matches!(
            stage,
            WelcomeStage::Collection | WelcomeStage::DefaultJob | WelcomeStage::Complete
        );
    let show_default_job = collects_diagnostics
        && readiness.collect_host_configured
        && matches!(stage, WelcomeStage::DefaultJob | WelcomeStage::Complete);

    JourneyModel {
        stage,
        user: config.user.unwrap_or_default(),
        workflow_value: workflow.map(workflow_value).unwrap_or_default().to_string(),
        processes_diagnostics,
        show_cluster,
        show_collection,
        show_default_job,
        show_complete: stage == WelcomeStage::Complete,
        collection_deferred: readiness.collection_deferred,
        keystore_ready: readiness.keystore_ready,
        keystore_unlocked,
        output_name,
        output_url,
        viewer_name,
        viewer_url,
        output_location,
        environment_output: runtime_output.declared,
        managed_local_stack: runtime_output.managed_local,
        container_runtime: detected_container_runtime().unwrap_or_default(),
        elasticsearch_ready: if runtime_output.configured {
            runtime_output.elasticsearch_ready
        } else {
            local_port_ready(9200).await
        },
        kibana_ready: if runtime_output.configured {
            runtime_output.kibana_ready
        } else {
            local_port_ready(5601).await
        },
        elasticsearch_assets: asset_status_name(runtime_output.elasticsearch_assets).to_string(),
        kibana_assets: asset_status_name(runtime_output.kibana_assets).to_string(),
        assets_installed: runtime_output.elasticsearch_assets == crate::setup::AssetStatus::Installed
            || runtime_output.kibana_assets == crate::setup::AssetStatus::Installed,
        collect_name,
        collect_url,
        default_job_name: config.job.default.unwrap_or_default(),
    }
}

fn detected_container_runtime() -> Option<String> {
    if let Some(runtime) = std::env::var("ESDIAG_CONTAINER_RUNTIME")
        .ok()
        .filter(|runtime| matches!(runtime.as_str(), "podman" | "docker"))
    {
        return Some(runtime);
    }
    ["podman", "docker"].into_iter().find_map(|runtime| {
        Command::new(runtime)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
            .then(|| runtime.to_string())
    })
}

async fn local_port_ready(port: u16) -> bool {
    tokio::time::timeout(
        Duration::from_millis(150),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .is_ok_and(|connection| connection.is_ok())
}

fn is_local_output_url(value: &str) -> bool {
    Url::parse(value)
        .ok()
        .and_then(|url| {
            url.host_str()
                .map(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"))
        })
        .unwrap_or(false)
}

async fn runtime_output_status(output_configured: bool) -> RuntimeOutputStatus {
    let output_url = std::env::var("ESDIAG_OUTPUT_URL").ok();
    let declared = output_url.is_some();
    let mut status = RuntimeOutputStatus {
        declared,
        managed_local: std::env::var("ESDIAG_CONTAINER_LOCAL_STACK").ok().as_deref() == Some("full")
            || output_url.as_deref().is_some_and(is_local_output_url),
        elasticsearch_url: output_url.unwrap_or_default(),
        kibana_url: std::env::var("ESDIAG_KIBANA_PUBLIC_URL")
            .or_else(|_| std::env::var("ESDIAG_KIBANA_URL"))
            .unwrap_or_default(),
        ..RuntimeOutputStatus::default()
    };
    if !declared && !output_configured {
        return status;
    }
    let deployment = match OutputDeployment::resolve(None, true) {
        Ok(deployment) => deployment,
        Err(err) => {
            tracing::warn!("Unable to resolve configured onboarding output: {err}");
            return status;
        }
    };
    let Some(kibana) = deployment.kibana else {
        return status;
    };
    status.elasticsearch_url = deployment
        .elasticsearch
        .concrete_url()
        .map(ToString::to_string)
        .unwrap_or(status.elasticsearch_url);
    status.kibana_url = kibana
        .concrete_url()
        .map(ToString::to_string)
        .unwrap_or(status.kibana_url);
    status.configured = true;
    status.managed_local |= is_local_output_url(&status.elasticsearch_url);
    let elasticsearch = match Uri::try_from(deployment.elasticsearch).and_then(Client::try_from) {
        Ok(client) => client,
        Err(err) => {
            tracing::warn!("Unable to construct environment Elasticsearch client: {err}");
            return status;
        }
    };
    let kibana = match Uri::try_from(kibana).and_then(Client::try_from) {
        Ok(client) => client,
        Err(err) => {
            tracing::warn!("Unable to construct environment Kibana client: {err}");
            return status;
        }
    };
    let connection_checks = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(elasticsearch.test_connection(), kibana.test_connection())
    })
    .await;
    if let Ok((elasticsearch_ready, kibana_ready)) = connection_checks {
        status.elasticsearch_ready = elasticsearch_ready.is_ok();
        status.kibana_ready = kibana_ready.is_ok();
    }
    if status.elasticsearch_ready && status.kibana_ready {
        (status.elasticsearch_assets, status.kibana_assets) = tokio::time::timeout(
            Duration::from_secs(3),
            crate::setup::asset_statuses(&elasticsearch, &kibana, Some(&status.kibana_url)),
        )
        .await
        .unwrap_or_default();
    }
    status
}

fn asset_status_name(status: crate::setup::AssetStatus) -> &'static str {
    match status {
        crate::setup::AssetStatus::Installed => "installed",
        crate::setup::AssetStatus::Missing => "missing",
        crate::setup::AssetStatus::Unknown => "unknown",
    }
}

fn workflow_value(workflow: OnboardingWorkflow) -> &'static str {
    match workflow {
        OnboardingWorkflow::CollectOnly => "collect-only",
        OnboardingWorkflow::ProcessExisting => "process-existing",
        OnboardingWorkflow::CollectAndProcess => "collect-and-process",
    }
}

fn workflow_processes_diagnostics() -> bool {
    ApplicationConfig::load()
        .ok()
        .and_then(|config| config.workflow)
        .is_some_and(OnboardingWorkflow::processes_diagnostics)
}

fn workflow_from_form(value: &str) -> Result<OnboardingWorkflow, eyre::Report> {
    match value {
        "collect-only" => Ok(OnboardingWorkflow::CollectOnly),
        "process-existing" => Ok(OnboardingWorkflow::ProcessExisting),
        "collect-and-process" => Ok(OnboardingWorkflow::CollectAndProcess),
        _ => Err(eyre::eyre!("Choose a supported onboarding workflow.")),
    }
}

fn stage_name(stage: WelcomeStage) -> &'static str {
    match stage {
        WelcomeStage::Identity => "identity",
        WelcomeStage::Output => "output",
        WelcomeStage::Collection => "collection",
        WelcomeStage::DefaultJob => "default-job",
        WelcomeStage::Complete => "complete",
    }
}

async fn save_output_stage(state: &Arc<ServerState>, form: OutputForm, keystore_password: &str) -> Result<(), String> {
    let output_url = url::Url::parse(&form.output_url).map_err(|err| format!("Invalid Elasticsearch URL: {err}"))?;
    let viewer_url = url::Url::parse(&form.viewer_url).map_err(|err| format!("Invalid Kibana URL: {err}"))?;
    let api_key = form.api_key.trim();
    if api_key.is_empty() {
        return Err("An output API key is required.".to_string());
    }
    if (KnownHost::get_known(&form.output_name).is_some()
        || KnownHost::get_known(&form.viewer_name).is_some()
        || list_secret_names(keystore_password).is_ok_and(|names| names.iter().any(|name| name == &form.output_name)))
        && form.replace.is_none()
    {
        return Err("A matching output name already exists. Confirm replacement to continue.".to_string());
    }

    let auth = SecretAuth::apikey(api_key);
    let output_candidate = onboarding::validation_host(output_url.clone(), Application::Elasticsearch, auth.clone())
        .map_err(|err| err.to_string())?;
    let viewer_candidate = onboarding::validation_host(viewer_url.clone(), Application::Kibana, auth.clone())
        .map_err(|err| err.to_string())?;
    let output_client = Client::try_from(Uri::try_from(output_candidate).map_err(|err| err.to_string())?)
        .map_err(|err| err.to_string())?;
    let viewer_client = Client::try_from(Uri::try_from(viewer_candidate).map_err(|err| err.to_string())?)
        .map_err(|err| err.to_string())?;
    let output_valid = output_client.test_connection().await.is_ok();
    let viewer_valid = viewer_client.test_connection().await.is_ok();
    if !output_valid || !viewer_valid {
        return Err(
            "The Elasticsearch and Kibana output endpoints must both validate before configuration is changed."
                .to_string(),
        );
    }

    onboarding::save_output_deployment(
        OutputDeploymentInput {
            output_name: form.output_name.clone(),
            output_url,
            viewer_name: form.viewer_name,
            viewer_url: viewer_url.clone(),
            secret_id: form.output_name.clone(),
            auth,
        },
        keystore_password,
    )
    .map_err(|err| err.to_string())?;

    let mut config = ApplicationConfig::load().map_err(|err| err.to_string())?;
    config.output.authenticated_on = Some(chrono::Utc::now().to_rfc3339());
    config.save().map_err(|err| err.to_string())?;

    let output = KnownHost::get_known(&form.output_name)
        .ok_or_else(|| "The saved Elasticsearch output host could not be reloaded.".to_string())?;
    let exporter = crate::data::with_scoped_keystore_password(keystore_password.to_string(), async move {
        Exporter::try_from(output).map_err(|err| err.to_string())
    })
    .await?;
    *state.exporter.write().await = exporter;
    *state.kibana_url.write().await = viewer_url.to_string();
    Ok(())
}

async fn install_output_assets_stage() -> Result<(), String> {
    let deployment = OutputDeployment::resolve(None, true).map_err(|err| err.to_string())?;
    let kibana = deployment
        .kibana
        .ok_or_else(|| "The environment output does not provide a Kibana endpoint.".to_string())?;
    let output_client = Client::try_from(Uri::try_from(deployment.elasticsearch).map_err(|err| err.to_string())?)
        .map_err(|err| err.to_string())?;
    let viewer_client =
        Client::try_from(Uri::try_from(kibana).map_err(|err| err.to_string())?).map_err(|err| err.to_string())?;
    crate::setup::assets(&output_client)
        .await
        .map_err(|err| err.to_string())?;
    crate::setup::ensure_agent_builder_license(&output_client)
        .await
        .map_err(|err| err.to_string())?;
    crate::setup::assets(&viewer_client)
        .await
        .map_err(|err| err.to_string())
}

async fn save_collection_stage(form: CollectionForm, keystore_password: &str) -> Result<(), String> {
    let collection_configured = onboarding::inspect()
        .map_err(|err| err.to_string())?
        .collect_host_configured;
    let replacing = KnownHost::get_known(&form.name).is_some();
    if (collection_configured || replacing) && form.replace.is_none() {
        return Err("A collection host is already configured. Confirm replacement to continue.".to_string());
    }
    let url = url::Url::parse(&form.url).map_err(|err| format!("Invalid collection URL: {err}"))?;
    let api_key = form.api_key.trim();
    if api_key.is_empty() {
        return Err("A collection API key is required.".to_string());
    }
    let auth = SecretAuth::apikey(api_key);
    let candidate = onboarding::validation_host(url.clone(), Application::Elasticsearch, auth.clone())
        .map_err(|err| err.to_string())?;
    let client =
        Client::try_from(Uri::try_from(candidate).map_err(|err| err.to_string())?).map_err(|err| err.to_string())?;
    client
        .test_connection()
        .await
        .map_err(|_| "The collection endpoint could not be validated. Configuration was not changed.".to_string())?;
    let input = CollectHostInput {
        name: form.name.clone(),
        app: Application::Elasticsearch,
        url,
        secret_id: Some(form.name),
        auth: Some(auth),
    };
    if replacing {
        onboarding::replace_collect_host(input, Some(keystore_password))
    } else {
        onboarding::save_collect_host(input, Some(keystore_password))
    }
    .map_err(|err| err.to_string())
}

fn save_default_job_stage(form: DefaultJobForm) -> Result<(), String> {
    let config = ApplicationConfig::load().map_err(|err| err.to_string())?;
    let job_exists = load_saved_jobs()
        .map_err(|err| err.to_string())?
        .contains_key(&form.name);
    if (config.job.default.is_some() || job_exists) && form.replace.is_none() {
        return Err("A default or same-named saved job already exists. Confirm replacement to continue.".to_string());
    }
    match config.workflow {
        Some(OnboardingWorkflow::CollectOnly) => {
            let job = crate::data::Job::builder()
                .collect_from(form.collect_host.clone())
                .map_err(|err| err.to_string())?
                .collect_to(format!("diagnostics/{}", form.collect_host))
                .map_err(|err| err.to_string())?;
            onboarding::save_default_job(form.name, job).map_err(|err| err.to_string())?;
        }
        Some(OnboardingWorkflow::CollectAndProcess) => {
            onboarding::save_default_processing_job(form.name, form.collect_host).map_err(|err| err.to_string())?;
        }
        Some(OnboardingWorkflow::ProcessExisting) | None => {
            return Err("This workflow does not use a default collection job.".to_string());
        }
    }
    Ok(())
}

async fn provision_local_output_stage(state: &Arc<ServerState>, keystore_password: &str) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|err| err.to_string())?;
    let status = Command::new(executable)
        .args([
            "local",
            "up",
            "--stack=core",
            "--start-native-service=false",
            "--open-browser=false",
            "--copy-password=false",
            "--persist-onboarding-output",
        ])
        .env("ESDIAG_KEYSTORE_PASSWORD", keystore_password)
        .status()
        .map_err(|err| format!("Could not start the local core stack: {err}"))?;
    if !status.success() {
        return Err(format!("Local core stack setup exited with {status}."));
    }

    let config = ApplicationConfig::load().map_err(|err| err.to_string())?;
    let output_name = config
        .output
        .default
        .ok_or_else(|| "Local stack setup did not configure an output deployment.".to_string())?;
    let output = KnownHost::get_known(&output_name)
        .ok_or_else(|| "Local stack setup did not save its Elasticsearch output host.".to_string())?;
    let viewer_name = output
        .viewer()
        .ok_or_else(|| "Local stack setup did not save its Kibana viewer.".to_string())?
        .to_string();
    let viewer = KnownHost::get_known(&viewer_name)
        .ok_or_else(|| "Local stack setup saved an unknown Kibana viewer.".to_string())?;
    let viewer_url = viewer
        .concrete_url()
        .ok_or_else(|| "Local stack setup saved an invalid Kibana URL.".to_string())?
        .to_string();
    let exporter = crate::data::with_scoped_keystore_password(keystore_password.to_string(), async move {
        Exporter::try_from(output).map_err(|err| err.to_string())
    })
    .await?;
    *state.exporter.write().await = exporter;
    *state.kibana_url.write().await = viewer_url;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        is_local_output_url, journey_model, runtime_output_status, workflow_from_form, workflow_processes_diagnostics,
    };
    use crate::{
        data::{OnboardingWorkflow, authenticate},
        onboarding::{defer_collection, save_user, save_workflow},
        server::{
            template::{Welcome, WelcomeStage},
            test_server_state,
        },
    };
    use askama::Template;

    #[test]
    fn accepts_only_supported_workflow_values() {
        assert_eq!(
            workflow_from_form("collect-and-process").expect("workflow"),
            OnboardingWorkflow::CollectAndProcess
        );
        assert!(workflow_from_form("unexpected").is_err());
    }

    #[test]
    fn loopback_output_is_local() {
        assert!(is_local_output_url("http://127.0.0.1:9200"));
        assert!(is_local_output_url("http://localhost:9200"));
        assert!(!is_local_output_url("https://output.example.test"));
    }

    #[tokio::test]
    async fn missing_onboarding_output_is_expected_state() {
        let _env = crate::TestEnv::new();

        let status = runtime_output_status(false).await;

        assert!(!status.declared);
        assert!(!status.configured);
    }

    #[tokio::test]
    async fn processing_workflow_advances_to_cluster_configuration() {
        let _env = crate::TestEnv::new();
        let state = test_server_state();
        assert_eq!(journey_model(&state).await.stage, WelcomeStage::Identity);

        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::ProcessExisting).expect("save workflow");

        assert_eq!(journey_model(&state).await.stage, WelcomeStage::Output);
        authenticate("password").expect("create keystore");
        assert_eq!(journey_model(&state).await.stage, WelcomeStage::Output);
    }

    #[tokio::test]
    async fn deferred_collection_skips_the_source_dependent_default_job() {
        let _env = crate::TestEnv::new();
        let state = test_server_state();
        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::CollectOnly).expect("save workflow");
        assert_eq!(journey_model(&state).await.stage, WelcomeStage::Collection);

        defer_collection().expect("defer collection");

        let model = journey_model(&state).await;
        assert_eq!(model.stage, WelcomeStage::Complete);
        assert!(!model.show_default_job);
        assert!(model.show_complete);
    }

    #[test]
    fn output_stage_never_renders_a_submitted_api_key() {
        let html = Welcome {
            stage: "output".to_string(),
            user: "operator@example.com".to_string(),
            workflow_value: "process-existing".to_string(),
            processes_diagnostics: true,
            show_cluster: true,
            keystore_ready: true,
            keystore_unlocked: true,
            message: "saved".to_string(),
            ..Welcome::default()
        }
        .render()
        .expect("render output stage");

        let local_action = html.find("/welcome/output/local").expect("local stack action");
        let local_start = html[..local_action].rfind("<form").expect("local stack form start");
        let local_end = local_action + html[local_action..].find("</form>").expect("local stack form end");
        let local_form = &html[local_start..local_end];
        assert!(html.contains(r#"aria-label="Choose a diagnostic cluster""#));
        assert!(html.contains(r#"data-show="$welcome.outputLocation === 'local'""#));
        assert!(html.contains(r#"data-show="$welcome.outputLocation === 'remote'""#));
        assert!(html.contains("Starting Elasticsearch and Kibana…"));
        assert!(html.contains(r#"class="spinner" aria-hidden="true""#));
        assert!(html.contains(r#"class="neutral">Unknown</badge>"#));
        assert!(!local_form.contains("api_key"));
        assert!(!local_form.contains("output_url"));
        assert!(html.contains(r#"type="password""#));
        assert!(!html.contains("api-key-that-must-not-escape"));
    }

    #[test]
    fn onboarding_messages_are_rendered_as_text_not_expressions() {
        let html = Welcome {
            message: "Secret 'Remote' was not found in keystore".to_string(),
            ..Welcome::default()
        }
        .render()
        .expect("render onboarding message");

        assert!(html.contains("Remote"));
        assert!(html.contains(r#"class="error" role="alert""#));
        assert!(!html.contains("data-signals:welcome.message"));
        assert!(!html.contains("data-text=\"$welcome.message\""));
    }

    #[test]
    fn cluster_configuration_reports_separate_asset_statuses() {
        let html = Welcome {
            stage: "output".to_string(),
            processes_diagnostics: true,
            show_cluster: true,
            keystore_ready: true,
            keystore_unlocked: true,
            output_name: "local".to_string(),
            output_location: "local".to_string(),
            elasticsearch_ready: true,
            kibana_ready: true,
            elasticsearch_assets: "installed".to_string(),
            kibana_assets: "missing".to_string(),
            assets_installed: true,
            ..Welcome::default()
        }
        .render()
        .expect("render asset status");

        assert!(html.contains("Elasticsearch Assets"));
        assert!(html.contains("Kibana Assets"));
        assert!(html.contains("Re-install Assets"));
        assert!(!html.contains("Reconfigure the current output"));
        assert!(html.contains(r#"class="success">Installed</badge>"#));
        assert!(html.contains(r#"class="warning">Missing</badge>"#));
        let remote_start = html
            .find(r#"data-show="$welcome.outputLocation === 'remote'"#)
            .expect("remote cluster section");
        let remote_end = remote_start + html[remote_start..].find("</section>").expect("remote section end");
        let remote_section = &html[remote_start..remote_end];
        assert!(remote_section.contains(r#"class="neutral">Unknown</badge>"#));
        assert!(!remote_section.contains(r#"class="success">Installed</badge>"#));
        assert!(!remote_section.contains("Re-install Assets"));
    }

    #[test]
    fn progressive_journey_renders_completed_steps_and_only_the_next_step() {
        let html = Welcome {
            stage: "output".to_string(),
            user: "operator@example.com".to_string(),
            workflow_value: "collect-and-process".to_string(),
            processes_diagnostics: true,
            show_cluster: true,
            keystore_ready: true,
            keystore_unlocked: true,
            ..Welcome::default()
        }
        .render()
        .expect("render progressive journey");

        assert!(html.contains("Diagnostic Tasks"));
        assert!(html.contains("Diagnostic Cluster"));
        assert!(html.contains("Cluster Configuration"));
        assert!(!html.contains("Diagnostic Source</h2>"));
        assert!(!html.contains("Default Diagnostic</h2>"));
        assert!(!html.contains("Ready</h2>"));
    }

    #[test]
    fn collection_stage_offers_to_add_a_source_later() {
        let html = Welcome {
            stage: "collection".to_string(),
            workflow_value: "collect-only".to_string(),
            show_collection: true,
            ..Welcome::default()
        }
        .render()
        .expect("render collection stage");

        assert!(html.contains("Add later"));
        assert!(html.contains("/welcome/collection/later"));
    }

    #[test]
    fn deferred_journey_does_not_claim_the_default_workflow_is_configured() {
        let html = Welcome {
            stage: "complete".to_string(),
            show_complete: true,
            collection_deferred: true,
            ..Welcome::default()
        }
        .render()
        .expect("render deferred journey");

        assert!(html.contains("Diagnostic source deferred"));
        assert!(html.contains("no diagnostic source or default workflow"));
        assert!(!html.contains("Your default diagnostic workflow is configured."));
    }

    #[test]
    fn completed_journey_keeps_every_applicable_step_editable() {
        let html = Welcome {
            stage: "complete".to_string(),
            user: "operator@example.com".to_string(),
            workflow_value: "collect-and-process".to_string(),
            processes_diagnostics: true,
            show_cluster: true,
            show_collection: true,
            show_default_job: true,
            show_complete: true,
            keystore_ready: true,
            keystore_unlocked: true,
            ..Welcome::default()
        }
        .render()
        .expect("render completed journey");

        assert!(html.contains("/welcome/identity"));
        assert!(html.contains("/welcome/output"));
        assert!(html.contains("/welcome/collection"));
        assert!(html.contains("/welcome/default-job"));
        assert!(html.contains("Ready</h2>"));
    }

    #[test]
    fn collection_only_workflow_cannot_configure_an_output() {
        let _env = crate::TestEnv::new();
        save_workflow(OnboardingWorkflow::CollectOnly).expect("save workflow");
        assert!(!workflow_processes_diagnostics());
    }
}
