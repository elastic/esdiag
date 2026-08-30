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

use elasticrc::{ConfigFile, ContextServiceReference, ResolvedAuth};
use std::{fs, process};

fn main() {
    let reference = ContextServiceReference::parse(".production.es").expect("context reference");
    let path = std::env::temp_dir().join(format!("elasticrc-external-consumer-{}.yml", process::id()));
    fs::write(
        &path,
        "current_context: production\ncontexts:\n  production:\n    elasticsearch:\n      url: https://example.invalid:9200\n      auth:\n        api_key: package-secret\n",
    )
    .expect("write config");
    let config = ConfigFile::load(&path).expect("load packaged config");
    let service = config
        .resolve_service(
            reference.context.as_deref().expect("named context"),
            reference.service,
        )
        .expect("resolve packaged service");

    assert_eq!(service.url.as_str(), "https://example.invalid:9200/");
    assert!(matches!(
        service.auth,
        ResolvedAuth::ApiKey(ref key) if key.expose_secret() == "package-secret"
    ));
    fs::remove_file(path).expect("remove config");
}
