use axum::http::StatusCode;

static DATASTAR_JS: &str = include_str!("datastar.js");
static DATASTAR_JS_MAP: &str = include_str!("datastar.js.map");
static ESDIAG_SVG: &str = include_str!("esdiag.svg");
static STYLE_CSS: &str = include_str!("style.css");

pub async fn datastar() -> (StatusCode, [(&'static str, &'static str); 1], &'static str) {
    (
        StatusCode::OK,
        [("Content-Type", "text/javascript")],
        DATASTAR_JS,
    )
}

pub async fn datastar_map() -> (StatusCode, [(&'static str, &'static str); 1], &'static str) {
    (
        StatusCode::OK,
        [("Content-Type", "application/json")],
        DATASTAR_JS_MAP,
    )
}

pub async fn logo() -> (StatusCode, [(&'static str, &'static str); 1], &'static str) {
    (
        StatusCode::OK,
        [("Content-Type", "image/svg+xml")],
        ESDIAG_SVG,
    )
}

pub async fn style() -> (StatusCode, [(&'static str, &'static str); 1], &'static str) {
    (StatusCode::OK, [("Content-Type", "text/css")], STYLE_CSS)
}
