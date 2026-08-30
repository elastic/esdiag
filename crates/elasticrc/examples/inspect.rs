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

use elasticrc::{ConfigFile, ResolvedAuth, ServiceKind};
use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("elasticrc example failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), elasticrc::Error> {
    let explicit_path = env::args_os().nth(1).map(PathBuf::from);
    let config = ConfigFile::load_with_options(explicit_path.as_deref(), None)?;
    let service = config.resolve_current_service(ServiceKind::Elasticsearch)?;

    println!("context: {}", config.current_context);
    println!("service: {}", service.kind);
    println!("url: {}", service.url);
    println!(
        "authentication: {}",
        match service.auth {
            ResolvedAuth::ApiKey(_) => "api_key",
            ResolvedAuth::Basic { .. } => "basic",
            ResolvedAuth::None => "none",
        }
    );
    Ok(())
}
