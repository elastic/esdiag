// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Phase-1 input resolution for stable Job references and runtime bindings.

use super::{Receiver, UploadServiceDownloader};
use crate::{
    data::{Application, HostRole, KnownHost, Uri},
    job::model::{BindingKey, Input},
};
use eyre::{Result, eyre};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone)]
enum RuntimeInput {
    Receiver {
        receiver: Receiver,
        application: Option<Application>,
        bundle_path: Option<PathBuf>,
    },
    Nested {
        parent: Receiver,
        path: String,
        application: Option<Application>,
    },
    Uri {
        uri: Uri,
        application: Option<Application>,
    },
}

/// Resolves the declaration in a Job into the receiver and optional local
/// bundle consumed by later stages.
#[derive(Clone, Default)]
pub struct InputResolver {
    bindings: HashMap<BindingKey, RuntimeInput>,
}

impl InputResolver {
    pub fn bind_receiver(&mut self, key: BindingKey, receiver: Receiver, application: Option<Application>) {
        self.bindings.insert(
            key,
            RuntimeInput::Receiver {
                receiver,
                application,
                bundle_path: None,
            },
        );
    }

    pub fn bind_bundle(
        &mut self,
        key: BindingKey,
        receiver: Receiver,
        bundle_path: PathBuf,
        application: Option<Application>,
    ) {
        self.bindings.insert(
            key,
            RuntimeInput::Receiver {
                receiver,
                application,
                bundle_path: Some(bundle_path),
            },
        );
    }

    pub fn bind_nested(
        &mut self,
        key: BindingKey,
        parent: Receiver,
        path: impl Into<String>,
        application: Option<Application>,
    ) {
        self.bindings.insert(
            key,
            RuntimeInput::Nested {
                parent,
                path: path.into(),
                application,
            },
        );
    }

    pub fn bind_uri(&mut self, key: BindingKey, uri: Uri, application: Option<Application>) {
        self.bindings.insert(key, RuntimeInput::Uri { uri, application });
    }

    pub async fn resolve(
        &self,
        input: &Input,
        materialize_remote: bool,
        require_local_bundle: bool,
    ) -> Result<ResolvedInput> {
        match input {
            Input::Collect { host, .. } => self.resolve_stable_collect(host),
            Input::CollectContext { target, .. } => self.resolve_context_collect(target),
            Input::CollectBinding { binding, .. } | Input::LoadBinding { binding } => {
                self.resolve_binding(binding, materialize_remote, require_local_bundle)
                    .await
            }
            Input::Load { uri } => self.resolve_stable_load(uri, materialize_remote).await,
        }
    }

    fn resolve_stable_collect(&self, host_name: &str) -> Result<ResolvedInput> {
        let hosts = KnownHost::parse_hosts_yml()?;
        let host_key = host_name.trim();
        let host = hosts
            .get(host_key)
            .cloned()
            .ok_or_else(|| eyre!("Host '{host_key}' referenced by job not found in hosts.yml"))?;
        let resolved = host.resolve()?;
        if !resolved.as_ref().has_role(HostRole::Collect) {
            return Err(eyre!("Collect host '{host_key}' is missing the collect role"));
        }
        let application = resolved.application();
        Ok(ResolvedInput::new(
            Receiver::try_from(resolved.into_known_host())?,
            Some(application),
            None,
            None,
        ))
    }

    fn resolve_context_collect(&self, target: &crate::data::ElasticContextTarget) -> Result<ResolvedInput> {
        let host = target.resolve_collect_host()?;
        if !host.has_role(HostRole::Collect) {
            return Err(eyre!(
                "Elastic CLI context target '{target}' is missing the collect role"
            ));
        }
        let application = host
            .app()
            .ok_or_else(|| eyre!("Elastic CLI context target '{target}' has no application"))?;
        Ok(ResolvedInput::new(
            Receiver::try_from(host)?,
            Some(application),
            None,
            None,
        ))
    }

    async fn resolve_stable_load(&self, uri: &Uri, materialize_remote: bool) -> Result<ResolvedInput> {
        match uri {
            Uri::File(path) => Ok(ResolvedInput::new(
                Receiver::try_from(uri.clone())?,
                None,
                Some(path.clone()),
                None,
            )),
            Uri::ServiceLink(url) if materialize_remote => {
                let bytes = UploadServiceDownloader::try_from(url.clone())?.download_bytes()?;
                let temp_dir =
                    std::env::temp_dir().join(format!("esdiag-input-{}", uuid::Uuid::new_v4().as_u64_pair().0));
                tokio::fs::create_dir_all(&temp_dir).await?;
                let bundle_path = temp_dir.join("diagnostic.zip");
                if let Err(error) = tokio::fs::write(&bundle_path, bytes).await {
                    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                    return Err(error.into());
                }
                let receiver = Receiver::try_from(Uri::File(bundle_path.clone()))?;
                Ok(ResolvedInput::new(
                    receiver,
                    None,
                    Some(bundle_path),
                    Some(TempInputCleanup(temp_dir)),
                ))
            }
            _ => Ok(ResolvedInput::new(Receiver::try_from(uri.clone())?, None, None, None)),
        }
    }

    async fn resolve_binding(
        &self,
        key: &BindingKey,
        materialize_remote: bool,
        require_local_bundle: bool,
    ) -> Result<ResolvedInput> {
        let binding = self
            .bindings
            .get(key)
            .ok_or_else(|| eyre!("Runtime input binding '{}' was not supplied", key.as_str()))?;
        match binding {
            RuntimeInput::Receiver {
                receiver,
                application,
                bundle_path,
            } => {
                if require_local_bundle && bundle_path.is_none() {
                    return Err(eyre!(
                        "Runtime input binding '{}' does not provide a local bundle",
                        key.as_str()
                    ));
                }
                Ok(ResolvedInput::new(
                    receiver.clone(),
                    *application,
                    bundle_path.clone(),
                    None,
                ))
            }
            RuntimeInput::Nested {
                parent,
                path,
                application,
            } => {
                if require_local_bundle {
                    return Err(eyre!(
                        "Nested runtime input binding '{}' is not a standalone local bundle",
                        key.as_str()
                    ));
                }
                Ok(ResolvedInput::new(
                    parent.clone_for_subdir(path)?,
                    *application,
                    None,
                    None,
                ))
            }
            RuntimeInput::Uri { uri, application } => {
                let mut resolved = self.resolve_stable_load(uri, materialize_remote).await?;
                resolved.application = *application;
                if require_local_bundle && resolved.bundle_path.is_none() {
                    return Err(eyre!(
                        "Runtime URI binding '{}' did not materialize a local bundle",
                        key.as_str()
                    ));
                }
                Ok(resolved)
            }
        }
    }
}

pub struct ResolvedInput {
    pub receiver: Receiver,
    pub application: Option<Application>,
    pub bundle_path: Option<PathBuf>,
    cleanup: Option<TempInputCleanup>,
}

impl ResolvedInput {
    fn new(
        receiver: Receiver,
        application: Option<Application>,
        bundle_path: Option<PathBuf>,
        cleanup: Option<TempInputCleanup>,
    ) -> Self {
        Self {
            receiver,
            application,
            bundle_path,
            cleanup,
        }
    }

    /// Keep a materialized remote input after execution and return its path.
    pub fn retain_bundle(&mut self) -> Option<PathBuf> {
        self.cleanup = None;
        self.bundle_path.clone()
    }

    pub(crate) fn from_bundle(receiver: Receiver, bundle_path: PathBuf) -> Self {
        Self::new(receiver, None, Some(bundle_path), None)
    }
}

struct TempInputCleanup(PathBuf);

impl Drop for TempInputCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn nested_binding_resolves_relative_to_parent_bundle() {
        let parent_dir = tempfile::tempdir().expect("parent bundle");
        std::fs::create_dir_all(parent_dir.path().join("children/es")).expect("child directory");
        let parent = Receiver::try_from(Uri::Directory(parent_dir.path().to_path_buf())).expect("parent receiver");
        let binding = BindingKey::try_new("child-es").expect("binding key");
        let mut resolver = InputResolver::default();
        resolver.bind_nested(binding.clone(), parent, "children/es", Some(Application::Elasticsearch));

        let resolved = resolver
            .resolve(&Input::LoadBinding { binding }, false, false)
            .await
            .expect("nested input resolves");

        assert!(resolved.receiver.is_bundle());
        assert_eq!(resolved.application, Some(Application::Elasticsearch));
        assert!(resolved.bundle_path.is_none());
    }

    #[tokio::test]
    async fn nested_binding_resolves_inside_archive_without_standalone_path() {
        let archive = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .expect("parent archive");
        let mut writer = zip::ZipWriter::new(archive.reopen().expect("archive handle"));
        writer
            .start_file("diagnostic_manifest.json", zip::write::SimpleFileOptions::default())
            .expect("parent manifest entry");
        writer
            .write_all(br#"{"product":"eck"}"#)
            .expect("write parent manifest");
        writer
            .start_file(
                "children/es/diagnostic_manifest.json",
                zip::write::SimpleFileOptions::default(),
            )
            .expect("child manifest entry");
        writer
            .write_all(
                br#"{
                    "timestamp":"2026-01-01T00:00:00Z",
                    "product":"elasticsearch",
                    "type":"elasticsearch_diagnostic",
                    "runner":"esdiag",
                    "version":"9.3.3",
                    "mode":"standard"
                }"#,
            )
            .expect("write child manifest");
        writer.finish().expect("finish archive");

        let parent = Receiver::try_from(Uri::File(archive.path().to_path_buf())).expect("archive receiver");
        let binding = BindingKey::try_new("archive-child-es").expect("binding key");
        let mut resolver = InputResolver::default();
        resolver.bind_nested(binding.clone(), parent, "children/es", Some(Application::Elasticsearch));

        let resolved = resolver
            .resolve(&Input::LoadBinding { binding }, false, false)
            .await
            .expect("archive-backed child resolves");
        let manifest = resolved
            .receiver
            .try_get_manifest()
            .await
            .expect("read nested child manifest");

        assert_eq!(manifest.application(), Some(Application::Elasticsearch));
        assert!(!archive.path().with_file_name("children/es").exists());
    }

    #[tokio::test]
    async fn binding_required_as_bundle_rejects_receiver_only_input() {
        let root = tempfile::tempdir().expect("bundle");
        let receiver = Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver");
        let binding = BindingKey::try_new("uploaded-bundle").expect("binding key");
        let mut resolver = InputResolver::default();
        resolver.bind_receiver(binding.clone(), receiver, None);

        let error = resolver
            .resolve(&Input::LoadBinding { binding }, false, true)
            .await
            .err()
            .expect("bundle path is required");

        assert!(error.to_string().contains("does not provide a local bundle"));
    }

    #[tokio::test]
    async fn missing_runtime_binding_is_reported_without_exposing_resources() {
        let binding = BindingKey::try_new("one-use-api-key").expect("binding key");
        let error = InputResolver::default()
            .resolve(&Input::LoadBinding { binding }, false, false)
            .await
            .err()
            .expect("missing binding");

        assert!(error.to_string().contains("one-use-api-key"));
        assert!(error.to_string().contains("was not supplied"));
    }
}
