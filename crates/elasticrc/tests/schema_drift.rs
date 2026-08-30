/*
 * Licensed to Elasticsearch B.V. under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch B.V. licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *	http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

use elasticrc::{ConfigFile, Error, ResolvedAuth, ServiceKind};
use std::fs;

#[test]
fn supported_elastic_cli_fixture_resolves_expected_services() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/elastic-cli-basic.yml");
    let config = ConfigFile::load(fixture).expect("fixture should load");

    let es = config
        .resolve_current_service(ServiceKind::Elasticsearch)
        .expect("current es service");
    let kb = config
        .resolve_current_service(ServiceKind::Kibana)
        .expect("current kb service");
    let cloud = config
        .resolve_service("diag", ServiceKind::Cloud)
        .expect("diag cloud service");

    assert_eq!(es.url.as_str(), "https://local-es.example:9200/");
    assert!(matches!(es.auth, ResolvedAuth::ApiKey(ref key) if key.expose_secret() == "local-api-key"));
    assert_eq!(kb.url.as_str(), "https://local-kb.example:5601/");
    assert_eq!(
        cloud.url.as_str(),
        "https://cloud.elastic.co/deployments/deployment-123"
    );
}

#[test]
fn platform_specific_keyring_resolver_rejects_unsupported_platform() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let path = tmp.path().join(".elasticrc.yml");
    let resolver = unsupported_platform_resolver();
    fs::write(
        &path,
        format!(
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $({resolver}:elastic-cli/prod-api-key)\n"
        ),
    )
    .expect("write config");

    let config = ConfigFile::load(&path).expect("load config");
    let err = config
        .resolve_current_service(ServiceKind::Elasticsearch)
        .expect_err("unsupported resolver should fail");

    assert!(matches!(err, Error::ResolverFailed { resolver: actual, .. } if actual == resolver));
}

#[cfg(target_os = "macos")]
fn unsupported_platform_resolver() -> &'static str {
    "secret_service"
}

#[cfg(target_os = "linux")]
fn unsupported_platform_resolver() -> &'static str {
    "credential_manager"
}

#[cfg(target_os = "windows")]
fn unsupported_platform_resolver() -> &'static str {
    "keychain"
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn unsupported_platform_resolver() -> &'static str {
    "keychain"
}
