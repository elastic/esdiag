// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

#[cfg(feature = "agent")]
#[derive(Debug)]
pub struct ProcessAskStdoutConflict;

#[cfg(feature = "agent")]
impl std::fmt::Display for ProcessAskStdoutConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "--ask cannot be used when process output is '-' because processed documents own stdout"
        )
    }
}

#[cfg(feature = "agent")]
impl std::error::Error for ProcessAskStdoutConflict {}
