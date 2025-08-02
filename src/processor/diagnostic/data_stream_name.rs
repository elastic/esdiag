use serde::Serialize;

#[derive(Clone, Serialize)]
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

impl ToString for DataStreamName {
    fn to_string(&self) -> String {
        format!("{}-{}-{}", self.r#type, self.dataset, self.namespace)
    }
}
