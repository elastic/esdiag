// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::context::ExecutionIdentity;
use crate::{
    data::{Application, Platform},
    processor::{CollectionResult, DiagnosticOutcome, DiagnosticReport},
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Collect,
    Load,
    Save,
    Process,
    Export,
    Send,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageStatus {
    Succeeded,
    Failed(String),
    Blocked(String),
    Skipped(String),
}

impl StageStatus {
    pub fn is_unsuccessful(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::Blocked(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageOutcome {
    pub stage: Stage,
    pub status: StageStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadResult {
    pub slug: String,
}

/// A child execution retained by its parent. The descriptor supplies the
/// nested-bundle context that belongs to the parent; the child result remains
/// the complete executor outcome, including its own identity and stage status.
pub struct ChildExecutionOutcome {
    pub path: String,
    pub execution: Box<ExecutionOutcome>,
    pub diagnostic_outcome: DiagnosticOutcome,
    pub application: Option<Application>,
    pub platform: Platform,
    pub runtime: Option<u128>,
}

impl ChildExecutionOutcome {
    pub fn job_id(&self) -> u64 {
        self.execution.identity.job_id
    }

    pub fn report(&self) -> Option<&DiagnosticReport> {
        self.execution.report.as_ref()
    }

    pub fn execution_error(&self) -> Option<&str> {
        self.execution.stages.iter().find_map(|stage| match &stage.status {
            StageStatus::Failed(error) | StageStatus::Blocked(error) => Some(error.as_str()),
            StageStatus::Succeeded | StageStatus::Skipped(_) => None,
        })
    }

    pub fn export_error(&self) -> Option<&str> {
        match self.execution.stage(Stage::Export) {
            Some(StageStatus::Failed(error) | StageStatus::Blocked(error)) => Some(error.as_str()),
            Some(StageStatus::Succeeded | StageStatus::Skipped(_)) | None => None,
        }
    }

    pub fn application(&self) -> Option<Application> {
        self.report()
            .and_then(|report| report.diagnostic.application)
            .or(self.application)
    }

    pub fn platform(&self) -> Platform {
        self.report()
            .map(|report| report.diagnostic.platform())
            .unwrap_or(self.platform)
    }
}

pub struct ExecutionOutcome {
    pub identity: ExecutionIdentity,
    pub stages: Vec<StageOutcome>,
    pub collection: Option<CollectionResult>,
    pub report: Option<DiagnosticReport>,
    pub children: Vec<ChildExecutionOutcome>,
    pub retained_bundle: Option<PathBuf>,
    pub upload: Option<UploadResult>,
}

impl ExecutionOutcome {
    pub fn new(identity: ExecutionIdentity) -> Self {
        Self {
            identity,
            stages: Vec::new(),
            collection: None,
            report: None,
            children: Vec::new(),
            retained_bundle: None,
            upload: None,
        }
    }

    pub fn stage(&self, stage: Stage) -> Option<&StageStatus> {
        self.stages
            .iter()
            .find(|outcome| outcome.stage == stage)
            .map(|outcome| &outcome.status)
    }

    pub fn succeeded(&self) -> bool {
        !self.stages.iter().any(|outcome| outcome.status.is_unsuccessful())
    }

    pub fn diagnostic_outcome(&self) -> Option<DiagnosticOutcome> {
        self.report.as_ref().map(DiagnosticReport::outcome)
    }

    pub(crate) fn record(&mut self, stage: Stage, status: StageStatus) {
        self.stages.push(StageOutcome { stage, status });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Queued,
    Started,
    Progress,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionEvent {
    pub identity: ExecutionIdentity,
    pub stage: Stage,
    pub lifecycle: Lifecycle,
    pub message: Option<String>,
}

impl ExecutionEvent {
    pub fn new(identity: ExecutionIdentity, stage: Stage, lifecycle: Lifecycle) -> Self {
        Self {
            identity,
            stage,
            lifecycle,
            message: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_selected_stage_makes_execution_unsuccessful() {
        let mut outcome = ExecutionOutcome::new(ExecutionIdentity::new(1, "test"));
        outcome.record(Stage::Process, StageStatus::Blocked("input failed".to_string()));

        assert!(!outcome.succeeded());
    }
}
