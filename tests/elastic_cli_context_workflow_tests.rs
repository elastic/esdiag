// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use esdiag::{
    data::{ApplicationConfig, Auth, ElasticContextTarget, ElasticOutputContext, OutputConfig, OutputDeployment},
    job::model::{ExportTarget, Input, Job, Process},
    processor::Identifiers,
};
use std::{fs, path::Path, sync::Mutex};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn contexts_resolve_independently_and_saved_jobs_reresolve_credentials() {
    let _guard = ENV_LOCK.lock().expect("environment lock");
    let home = tempfile::TempDir::new().expect("temp home");
    let state_dir = home.path().join(".esdiag");
    fs::create_dir_all(&state_dir).expect("state directory");
    let config_path = home.path().join(".elasticrc.yml");
    write_config(&config_path);
    set_env(home.path(), &config_path, &state_dir);

    let mut application = ApplicationConfig::new();
    application.output = OutputConfig {
        elastic_context: Some(ElasticOutputContext {
            context: "monitoring".to_string(),
            config_file: Some(config_path.clone()),
        }),
        ..OutputConfig::default()
    };
    application.save().expect("save output context");

    let active = ElasticContextTarget::parse(".es")
        .expect("parse active")
        .expect("active target");
    let active_host = active.resolve_collect_host().expect("resolve active input");
    assert_eq!(
        active_host.concrete_url().map(url::Url::as_str),
        Some("https://active-prod.example:9200/")
    );
    assert!(matches!(
        active_host.get_auth().expect("active auth"),
        Auth::Apikey(key) if key.expose_secret() == "active-prod-key"
    ));

    let configured = OutputDeployment::resolve(None, false).expect("configured output");
    assert_eq!(
        configured.elasticsearch.concrete_url().map(url::Url::as_str),
        Some("https://monitoring.example:9200/")
    );
    assert!(matches!(
        configured.elasticsearch_auth,
        Auth::Apikey(ref key) if key.expose_secret() == "monitoring-key-one"
    ));

    let explicit = OutputDeployment::resolve(Some(".incident.es"), false).expect("explicit output");
    assert_eq!(
        explicit.elasticsearch.concrete_url().map(url::Url::as_str),
        Some("https://incident.example:9200/")
    );

    let prod = ElasticContextTarget::parse(".prod.es")
        .expect("parse prod")
        .expect("prod");
    let customer = ElasticContextTarget::parse(".customer.es")
        .expect("parse customer")
        .expect("customer");
    assert_eq!(
        prod.resolve_collect_host()
            .expect("prod host")
            .concrete_url()
            .map(url::Url::as_str),
        Some("https://prod.example:9200/")
    );
    assert_eq!(
        customer
            .resolve_collect_host()
            .expect("customer host")
            .concrete_url()
            .map(url::Url::as_str),
        Some("https://customer.example:9200/")
    );

    let missing = ElasticContextTarget::parse(".broken.kb")
        .expect("parse missing service")
        .expect("missing target")
        .resolve_collect_host()
        .expect_err("missing Kibana must fail");
    assert!(missing.to_string().contains("service 'kibana'"));

    let job = Job::try_new(
        Identifiers::default(),
        Input::CollectContext {
            target: prod,
            diagnostic_type: "standard".to_string(),
            include: None,
            exclude: None,
        },
        None,
        Some(Process {
            selection: None,
            export: ExportTarget::ElasticContext {
                target: ElasticContextTarget::parse(".monitoring.es")
                    .expect("parse monitoring")
                    .expect("monitoring target"),
            },
        }),
        None,
    )
    .expect("context job");
    let saved = yaml_serde::to_string(&job).expect("serialize job");
    assert!(!saved.contains("monitoring-key-one"));

    unsafe {
        std::env::set_var("MONITORING_KEY", "monitoring-key-two");
    }
    let loaded: Job = yaml_serde::from_str(&saved).expect("deserialize job");
    let target = match loaded.process().map(|process| &process.export) {
        Some(ExportTarget::ElasticContext { target }) => target,
        _ => panic!("expected symbolic output context"),
    };
    let rotated = OutputDeployment::from_elastic_target(target, false).expect("re-resolve saved output");
    assert!(matches!(
        rotated.elasticsearch_auth,
        Auth::Apikey(ref key) if key.expose_secret() == "monitoring-key-two"
    ));
}

#[cfg(unix)]
#[test]
fn direct_process_resolves_context_command_once() {
    use std::{os::unix::fs::PermissionsExt, process::Command};

    let home = tempfile::TempDir::new().expect("temp home");
    let marker = home.path().join("resolver-count");
    let resolver = home.path().join("resolve-key");
    fs::write(
        &resolver,
        format!(
            "#!/bin/sh\ncount=0\nif test -f \"{}\"; then count=$(cat \"{}\"); fi\nprintf '%s\\n' \"$((count + 1))\" > \"{}\"\nprintf context-key\n",
            marker.display(),
            marker.display(),
            marker.display()
        ),
    )
    .expect("write resolver");
    fs::set_permissions(&resolver, fs::Permissions::from_mode(0o755)).expect("resolver permissions");
    let config = home.path().join(".elasticrc.yml");
    fs::write(
        &config,
        format!(
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: http://127.0.0.1:9\n      auth:\n        api_key: $(cmd:{})\n",
            resolver.display()
        ),
    )
    .expect("write config");
    let state_dir = home.path().join(".esdiag");
    fs::create_dir_all(&state_dir).expect("state directory");

    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "process",
            ".prod.es",
            home.path().join("output.ndjson").to_str().expect("output path"),
        ])
        .env("ESDIAG_ELASTIC_CLI", "1")
        .env("ELASTIC_CLI_CONFIG_FILE", config)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_HOSTS", state_dir.join("hosts.yml"))
        .output()
        .expect("run process");

    assert!(!output.status.success(), "unreachable test endpoint must fail");
    assert_eq!(fs::read_to_string(marker).expect("resolver count").trim(), "1");
}

#[cfg(unix)]
#[test]
fn direct_process_resolves_output_context_command_once() {
    use std::{os::unix::fs::PermissionsExt, process::Command};

    let home = tempfile::TempDir::new().expect("temp home");
    let marker = home.path().join("output-resolver-count");
    let resolver = home.path().join("resolve-output-url");
    fs::write(
        &resolver,
        format!(
            "#!/bin/sh\ncount=0\nif test -f \"{}\"; then count=$(cat \"{}\"); fi\nprintf '%s\\n' \"$((count + 1))\" > \"{}\"\nprintf http://127.0.0.1:9\n",
            marker.display(),
            marker.display(),
            marker.display()
        ),
    )
    .expect("write resolver");
    fs::set_permissions(&resolver, fs::Permissions::from_mode(0o755)).expect("resolver permissions");
    let config = home.path().join(".elasticrc.yml");
    fs::write(
        &config,
        format!(
            "current_context: monitoring\ncontexts:\n  monitoring:\n    elasticsearch:\n      url: $(cmd:{})\n",
            resolver.display()
        ),
    )
    .expect("write config");
    let state_dir = home.path().join(".esdiag");
    fs::create_dir_all(&state_dir).expect("state directory");
    let archive = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/archives/elasticsearch-api-diagnostics-9.3.3.zip"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", archive, ".monitoring.es"])
        .env("ESDIAG_ELASTIC_CLI", "1")
        .env("ELASTIC_CLI_CONFIG_FILE", config)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_HOSTS", state_dir.join("hosts.yml"))
        .output()
        .expect("run process");

    assert!(!output.status.success(), "unreachable test endpoint must fail");
    assert_eq!(fs::read_to_string(marker).expect("resolver count").trim(), "1");
}

fn write_config(path: &Path) {
    fs::write(
        path,
        r#"current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://prod.example:9200
  customer:
    elasticsearch:
      url: https://customer.example:9200
  monitoring:
    elasticsearch:
      url: https://monitoring.example:9200
      auth:
        api_key: $(env:MONITORING_KEY)
  incident:
    elasticsearch:
      url: https://incident.example:9200
  broken:
    elasticsearch:
      url: https://broken.example:9200
"#,
    )
    .expect("write Elastic CLI config");
}

fn set_env(home: &Path, config: &Path, state_dir: &Path) {
    unsafe {
        std::env::set_var("HOME", home);
        std::env::set_var("USERPROFILE", home);
        std::env::set_var("ESDIAG_HOSTS", state_dir.join("hosts.yml"));
        std::env::set_var("ELASTIC_CLI_CONFIG_FILE", config);
        std::env::set_var("ESDIAG_ELASTIC_CLI", "1");
        std::env::set_var("ELASTIC_ES_URL", "https://active-prod.example:9200");
        std::env::set_var("ELASTIC_ES_API_KEY", "active-prod-key");
        std::env::set_var("MONITORING_KEY", "monitoring-key-one");
        std::env::remove_var("ESDIAG_OUTPUT_URL");
        std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
        std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
        std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
    }
}
