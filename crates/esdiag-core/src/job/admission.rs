// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use std::fmt;

/// A job was refused because a concurrency cap is full.
#[derive(Debug)]
pub enum JobAdmissionError {
    GlobalCapReached { active: usize, cap: usize },
    OwnerCapReached { owner: String, active: usize, cap: usize },
}

impl fmt::Display for JobAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GlobalCapReached { active, cap } => {
                write!(f, "Service mode concurrent job cap reached ({active}/{cap})")
            }
            Self::OwnerCapReached { owner, active, cap } => {
                write!(
                    f,
                    "Service mode per-owner concurrent job cap reached for {owner} ({active}/{cap})"
                )
            }
        }
    }
}

impl std::error::Error for JobAdmissionError {}
