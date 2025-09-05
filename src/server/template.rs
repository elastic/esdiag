use askama::Template;

#[derive(Template)]
#[template(path = "error.html")]
pub struct Error<'e> {
    pub id: &'e str,
    pub error: &'e str,
    pub message: &'e str,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index {
    pub auth_header: bool,
    pub debug: bool,
    pub exporter: String,
    pub kibana_url: String,
    pub key_id: Option<u64>,
    pub link_id: Option<u64>,
    pub upload_id: Option<u64>,
    pub stats: String,
    pub user: String,
    pub user_initial: char,
    pub version: String,
}

#[derive(Template)]
#[template(path = "job/completed.html")]
pub struct JobCompleted<'a> {
    pub job_id: u64,
    pub diagnostic_id: &'a str,
    pub docs_created: &'a u32,
    pub duration: &'a str,
    pub source: &'a str,
    pub kibana_link: &'a str,
    pub product: &'a str,
}

#[derive(Template)]
#[template(path = "job/failed.html")]
pub struct JobFailed<'a> {
    pub job_id: u64,
    pub error: &'a str,
    pub source: &'a str,
}

#[derive(Template)]
#[template(path = "job/processing.html")]
pub struct JobProcessing<'a> {
    pub job_id: u64,
    pub source: &'a str,
}
