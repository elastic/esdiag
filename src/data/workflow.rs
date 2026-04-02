use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Workflow {
    pub collect: CollectStage,
    pub process: ProcessStage,
    pub send: SendStage,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct CollectStage {
    pub mode: CollectMode,
    pub source: CollectSource,
    #[serde(default)]
    pub known_host: String,
    #[serde(default)]
    pub diagnostic_type: String,
    #[serde(default)]
    pub save: bool,
    #[serde(default)]
    pub save_dir: String,
}

impl Default for CollectStage {
    fn default() -> Self {
        Self {
            mode: CollectMode::Collect,
            source: CollectSource::KnownHost,
            known_host: String::new(),
            diagnostic_type: "standard".to_string(),
            save: false,
            save_dir: String::new(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ProcessStage {
    pub mode: ProcessMode,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub product: String,
    #[serde(default)]
    pub diagnostic_type: String,
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub selected: String,
}

impl Default for ProcessStage {
    fn default() -> Self {
        Self {
            mode: ProcessMode::Process,
            enabled: true,
            product: "elasticsearch".to_string(),
            diagnostic_type: "standard".to_string(),
            advanced: false,
            selected: String::new(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SendStage {
    pub mode: SendMode,
    #[serde(default)]
    pub remote_target: String,
    #[serde(default)]
    pub local_target: String,
    #[serde(default)]
    pub local_directory: String,
}

impl Default for SendStage {
    fn default() -> Self {
        Self {
            mode: SendMode::Remote,
            remote_target: String::new(),
            local_target: String::new(),
            local_directory: String::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CollectMode {
    Collect,
    Upload,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CollectSource {
    KnownHost,
    ApiKey,
    ServiceLink,
    UploadFile,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessMode {
    Process,
    Forward,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SendMode {
    Remote,
    Local,
}

#[cfg(test)]
mod tests {
    use super::{CollectMode, CollectSource, SendMode, Workflow};
    use serde_json::json;

    #[test]
    fn workflow_deserializes_with_missing_process_and_send_stages() {
        let workflow: Workflow = serde_json::from_value(json!({
            "collect": {
                "mode": "upload",
                "source": "upload-file",
                "save": false
            }
        }))
        .expect("workflow should deserialize with defaults");

        assert!(workflow.collect.mode == CollectMode::Upload);
        assert!(workflow.collect.source == CollectSource::UploadFile);
        assert!(workflow.process.enabled);
        assert!(workflow.send.mode == SendMode::Remote);
    }
}
