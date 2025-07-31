use askama::Template;

#[derive(Template)]
#[template(path = "error.html")]
pub struct Error<'e> {
    pub id: &'e str,
    pub error: &'e str,
    pub message: &'e str,
}

impl Error<'_> {
    pub fn new(id: &str, error: &str, message: &str) -> String {
        let error = Error { id, error, message };
        match error.render() {
            Ok(html) => html,
            Err(err) => format!(
                r#"<div id="error-render" class="error"><h3>🛑 Failed rendering error template</h3><p>{err}</p></div>"#
            ),
        }
    }
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

impl Status<'_> {
    pub fn new(class: &str, message: &str) -> String {
        let status = Status { class, message };
        match status.render() {
            Ok(html) => html,
            Err(err) => Error::new(
                "error-render",
                "Failed rendering status template",
                &err.to_string(),
            ),
        }
    }
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
#[template(path = "job_completed.html")]
pub struct JobCompleted<'a> {
    pub job_id: u64,
    pub diagnostic_id: &'a str,
    pub docs_created: &'a u32,
    pub filename: &'a str,
    pub kibana_link: &'a str,
    pub product: &'a str,
}

#[derive(Template)]
#[template(path = "job_failed.html")]
pub struct JobFailed<'a> {
    pub job_id: u64,
    pub error: &'a str,
    pub filename: &'a str,
}

#[derive(Template)]
#[template(path = "job_processing.html")]
pub struct JobProcessing<'a> {
    pub job_id: u64,
    pub filename: &'a str,
}
