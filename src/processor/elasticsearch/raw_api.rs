use crate::processor::DataSource;

pub struct RawApi {
    name: String,
}

impl RawApi {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

// But wait, the DataSource trait requires a STATIC name!
