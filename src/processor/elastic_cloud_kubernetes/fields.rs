// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Document fragments shared by several ECK streams.

use super::{
    labels,
    quantity::Quantities,
    resources::{Condition, ResourceRequirements},
};
use serde::Serialize;
use std::collections::BTreeMap;

/// `kubernetes.*` join keys.
#[derive(Clone, Default, Serialize)]
pub struct KubernetesRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pod: Option<Named>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<Named>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl KubernetesRef {
    pub fn namespace(namespace: &str) -> Self {
        Self {
            namespace: Some(namespace.to_string()),
            ..Self::default()
        }
    }

    pub fn pod(mut self, pod: Option<&str>) -> Self {
        self.pod = pod.map(Named::from);
        self
    }

    pub fn node(mut self, node: Option<&str>) -> Self {
        self.node = node.map(Named::from);
        self
    }

    pub fn labels(mut self, all_labels: &BTreeMap<String, String>) -> Self {
        self.labels = labels::indexed_labels(all_labels);
        self
    }
}

#[derive(Clone, Serialize)]
pub struct Named {
    pub name: String,
}

impl From<&str> for Named {
    fn from(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

/// `eck.*` ownership keys.
#[derive(Clone, Default, Serialize)]
pub struct EckRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
}

impl EckRef {
    pub fn resource(kind: &str, name: &str) -> Self {
        Self {
            resource: Some(ResourceKey {
                kind: kind.to_string(),
                name: name.to_string(),
            }),
            r#type: None,
        }
    }

    /// Ownership of a pod or claim from its ECK labels.
    pub fn from_labels(all_labels: &BTreeMap<String, String>) -> Self {
        Self {
            resource: labels::owning_resource(all_labels).map(|(kind, name)| ResourceKey {
                kind: kind.to_string(),
                name,
            }),
            r#type: all_labels.get(labels::TYPE_LABEL).cloned(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.resource.is_none() && self.r#type.is_none()
    }
}

#[derive(Clone, Serialize)]
pub struct ResourceKey {
    pub kind: String,
    pub name: String,
}

/// Conditions keyed by type, so `condition.MemoryPressure: "True"` is a
/// single-field query instead of a cross-element match on the array.
pub fn condition_map(conditions: &[Condition]) -> BTreeMap<String, String> {
    conditions
        .iter()
        .filter_map(|condition| Some((condition.r#type.clone()?, condition.status.clone()?)))
        .collect()
}

/// Parsed container requests and limits.
#[derive(Clone, Default, Serialize)]
pub struct Resources {
    #[serde(skip_serializing_if = "Quantities::is_empty")]
    pub requests: Quantities,
    #[serde(skip_serializing_if = "Quantities::is_empty")]
    pub limits: Quantities,
}

impl Resources {
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty() && self.limits.is_empty()
    }
}

impl From<&ResourceRequirements> for Resources {
    fn from(resources: &ResourceRequirements) -> Self {
        Self {
            requests: Quantities::from_raw(resources.requests.as_ref()),
            limits: Quantities::from_raw(resources.limits.as_ref()),
        }
    }
}
