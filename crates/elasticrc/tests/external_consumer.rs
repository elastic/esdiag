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

use elasticrc::{ConfigFile, ContextServiceReference, ResolvedAuth, ServiceKind};

#[test]
fn public_api_resolves_context_without_esdiag_types() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/elastic-cli-basic.yml");
    let reference = ContextServiceReference::parse(".local.es").expect("context reference");
    let config = ConfigFile::load(fixture).expect("load config");
    let service = config
        .resolve_service(
            reference.context.as_deref().expect("named context"),
            ServiceKind::Elasticsearch,
        )
        .expect("resolve service");

    assert_eq!(service.url.as_str(), "https://local-es.example:9200/");
    assert!(matches!(service.auth, ResolvedAuth::ApiKey(_)));
}
