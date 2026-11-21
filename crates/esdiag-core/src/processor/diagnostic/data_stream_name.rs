// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct DataStreamName {
    dataset: String,
    namespace: String,
    r#type: String,
}

impl From<&str> for DataStreamName {
    fn from(name: &str) -> Self {
        let terms: Vec<&str> = name.split('-').collect();
        DataStreamName {
            r#type: terms[0].to_string(),
            dataset: terms[1].to_string(),
            namespace: terms[2].to_string(),
        }
    }
}

impl std::fmt::Display for DataStreamName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{}", self.r#type, self.dataset, self.namespace)
    }
}
