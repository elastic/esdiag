// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    model::{BindingKey, ExportTarget},
    outcome::ExecutionEvent,
};
use crate::{
    data::{Application, HostRole, KnownHost, Platform, Uri},
    elastic_upload_service::{BundleSender, BundleSending},
    exporter::DocumentExporter,
    processor::new_job_id,
    receiver::InputResolver,
};
use eyre::{Result, eyre};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionIdentity {
    pub job_id: u64,
    pub owner: String,
    pub parent_job_id: Option<u64>,
}

impl ExecutionIdentity {
    pub fn new(job_id: u64, owner: impl Into<String>) -> Self {
        Self {
            job_id,
            owner: owner.into(),
            parent_job_id: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RetentionPolicy {
    #[default]
    Ephemeral,
    RetainLoadedBundle,
}

pub trait ExecutionObserver: Send + Sync {
    fn observe(&self, event: &ExecutionEvent);
}

#[derive(Default)]
struct NoopObserver;

impl ExecutionObserver for NoopObserver {
    fn observe(&self, _event: &ExecutionEvent) {}
}

#[derive(Clone)]
pub struct ExecutionContext {
    pub inputs: InputResolver,
    document_exporters: HashMap<BindingKey, DocumentExporter>,
    pub sender: Arc<dyn BundleSending>,
    pub identity: ExecutionIdentity,
    pub retention: RetentionPolicy,
    pub inherited_platform: Option<Platform>,
    pub child_depth: u8,
    observer: Arc<dyn ExecutionObserver>,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            inputs: InputResolver::default(),
            document_exporters: HashMap::new(),
            sender: Arc::new(BundleSender::default()),
            identity: ExecutionIdentity::new(new_job_id(), "CLI"),
            retention: RetentionPolicy::Ephemeral,
            inherited_platform: None,
            child_depth: 0,
            observer: Arc::new(NoopObserver),
        }
    }
}

impl ExecutionContext {
    pub fn with_identity(mut self, identity: ExecutionIdentity) -> Self {
        self.identity = identity;
        self
    }

    pub fn with_observer(mut self, observer: Arc<dyn ExecutionObserver>) -> Self {
        self.observer = observer;
        self
    }

    pub fn with_sender(mut self, sender: impl BundleSending + 'static) -> Self {
        self.sender = Arc::new(sender);
        self
    }

    pub fn with_retention(mut self, retention: RetentionPolicy) -> Self {
        self.retention = retention;
        self
    }

    pub fn bind_document_exporter(&mut self, key: BindingKey, exporter: DocumentExporter) {
        self.document_exporters.insert(key, exporter);
    }

    pub(crate) fn observe(&self, event: ExecutionEvent) {
        self.observer.observe(&event);
    }

    pub(crate) fn resolve_document_exporter(&self, target: &ExportTarget) -> Result<DocumentExporter> {
        match target {
            ExportTarget::Binding { binding } => self.document_exporters.get(binding).cloned().ok_or_else(|| {
                eyre!(
                    "Runtime document exporter binding '{}' was not supplied",
                    binding.as_str()
                )
            }),
            ExportTarget::KnownHost { name } => {
                let hosts = KnownHost::parse_hosts_yml()?;
                let host_key = name.trim();
                let host = hosts
                    .get(host_key)
                    .cloned()
                    .ok_or_else(|| eyre!("Export host '{host_key}' not found in hosts.yml"))?;
                if !host.has_role(HostRole::Send) {
                    return Err(eyre!("Export host '{host_key}' is missing the send role"));
                }
                if host.app() != Some(Application::Elasticsearch) {
                    return Err(eyre!("Export host '{host_key}' must be an Elasticsearch host"));
                }
                DocumentExporter::try_from(Uri::try_from(host)?)
            }
            ExportTarget::ElasticContext { target } => {
                let deployment = crate::data::OutputDeployment::from_elastic_target(target, false)?;
                DocumentExporter::try_from(Uri::try_from(deployment.elasticsearch)?)
            }
            ExportTarget::File { path } => DocumentExporter::try_from(Uri::File(path.clone())),
            ExportTarget::Directory { output_dir } => DocumentExporter::try_from(Uri::Directory(output_dir.clone())),
            ExportTarget::Stdout => DocumentExporter::try_from(Uri::Stream),
        }
    }

    pub fn child(&self, inherited_platform: Platform) -> Result<Self> {
        if self.child_depth >= 1 {
            return Err(eyre!("Included diagnostic fan-out is limited to one child level"));
        }
        let mut child = self.clone();
        child.identity = ExecutionIdentity {
            job_id: next_distinct_job_id(self.identity.job_id, new_job_id),
            owner: self.identity.owner.clone(),
            parent_job_id: Some(self.identity.job_id),
        };
        child.inherited_platform = Some(inherited_platform);
        child.child_depth += 1;
        Ok(child)
    }
}

fn next_distinct_job_id(parent_job_id: u64, mut next_job_id: impl FnMut() -> u64) -> u64 {
    loop {
        let job_id = next_job_id();
        if job_id != parent_job_id {
            return job_id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_context_inherits_owner_platform_and_parent_identity() {
        let parent = ExecutionContext::default().with_identity(ExecutionIdentity::new(17, "alice@example.com"));

        let child = parent.child(Platform::ECK).expect("child context");

        assert_ne!(child.identity.job_id, parent.identity.job_id);
        assert_eq!(child.identity.owner, parent.identity.owner);
        assert_eq!(child.identity.parent_job_id, Some(parent.identity.job_id));
        assert_eq!(child.inherited_platform, Some(Platform::ECK));
        assert_eq!(child.child_depth, 1);
        assert!(child.child(Platform::ECK).is_err());
    }

    #[test]
    fn child_job_id_skips_parent_identity_collision() {
        let mut allocated_ids = [42, 43].into_iter();

        let child_id = next_distinct_job_id(42, || allocated_ids.next().expect("allocated ID"));

        assert_eq!(child_id, 43);
    }
}
