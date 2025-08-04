use askama::Template;

#[derive(Template)]
#[template(path = "error.html")]
pub struct Error<'e> {
    pub id: &'e str,
    pub error: &'e str,
    pub message: &'e str,
}

#[derive(Template)]
#[template(
    source = r#"<div id="current-status" class="status-box {{ class }}"> ✅ {{ message }}</div>"#,
    ext = "html"
)]
pub struct Status<'s> {
    pub class: &'s str,
    pub message: &'s str,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index {
    pub debug: bool,
    pub exporter: String,
    pub kibana_url: String,
    pub user: String,
    pub user_initial: char,
    pub version: String,
}

#[derive(Template)]
#[template(
    source = r#"<div id="ready-status" class="status-box ready">▶️ Ready, exporting to {{ exporter }}</div>"#,
    ext = "html"
)]
pub struct StatusReady {
    pub exporter: String,
}

#[derive(Template)]
#[template(
    source = r#"<div id="ready-status" class="status-box ready">🔄 Processing job: {{ job_id }}</div>"#,
    ext = "html"
)]
pub struct StatusProcessing {
    pub job_id: String,
}

#[derive(Template)]
#[template(
    source = r#"<div id="ready-status" class="status-box ready">⏸️ Queue: {{ queue_size }}</div>"#,
    ext = "html"
)]

pub struct StatusQueue {
    pub queue_size: usize,
}

#[derive(Template)]
#[template(
    source = r#"<div id="current-status" class="status-box processing"><div class="spinner"></div><span><b>Processing:</b> {{ filename }}</span></div>"#,
    ext = "html"
)]
pub struct CurrentStatusProcessing {
    pub filename: String,
}

#[derive(Template)]
#[template(
    source = r#"<div id="current-status" class="status-box hidden"></div>"#,
    ext = "html"
)]
pub struct CurrentStatusIdle {}

#[derive(Template)]
#[template(path = "job/completed.html")]
pub struct JobCompleted<'a> {
    pub job_id: u64,
    pub diagnostic_id: &'a str,
    pub docs_created: &'a u32,
    pub filename: &'a str,
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
    pub filename: &'a str,
}
