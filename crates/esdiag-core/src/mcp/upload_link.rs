// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::Uri;
use url::{Host, Url};

const UPLOAD_HOST: &str = "upload.elastic.co";

/// An Elastic Upload Service download link validated without touching local
/// state: no saved hosts, keystore, or filesystem.
pub struct UploadLink(Uri);

impl UploadLink {
    /// Accepts only an absolute `https` URL on `upload.elastic.co` with a
    /// path and no userinfo, port, query, or fragment, plus a non-empty
    /// token. Failure messages never repeat the URL or the token.
    pub fn parse(url: &str, token: &str) -> Result<Self, &'static str> {
        const NOT_UPLOAD: &str = "url must be an absolute https URL on upload.elastic.co";
        if token.is_empty() || token.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("token must be a non-empty Elastic Upload Service download token");
        }
        let mut url = Url::parse(url).map_err(|_| NOT_UPLOAD)?;
        let upload_service =
            url.scheme() == "https" && url.host() == Some(Host::Domain(UPLOAD_HOST)) && url.port().is_none();
        if !upload_service && !is_test_origin(&url) {
            return Err(NOT_UPLOAD);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("url must not embed credentials; pass the download token as `token`");
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err("url must not include a query or fragment");
        }
        if matches!(url.path(), "" | "/") {
            return Err("url must name an upload");
        }
        if url.set_username("token").is_err() || crate::data::set_upload_token(&mut url, token).is_err() {
            return Err(NOT_UPLOAD);
        }
        Ok(Self(Uri::ServiceLink(url)))
    }

    pub fn into_uri(self) -> Uri {
        self.0
    }
}

#[cfg(any(test, feature = "test-support"))]
static TEST_ORIGIN: std::sync::Mutex<Option<url::Origin>> = std::sync::Mutex::new(None);

/// Also accepts links on `origin`, a local mock of the Upload Service.
#[cfg(any(test, feature = "test-support"))]
pub fn allow_test_origin(origin: Option<&Url>) {
    *TEST_ORIGIN.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = origin.map(Url::origin);
}

#[cfg(any(test, feature = "test-support"))]
fn is_test_origin(url: &Url) -> bool {
    TEST_ORIGIN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .is_some_and(|origin| *origin == url.origin())
}

#[cfg(not(any(test, feature = "test-support")))]
fn is_test_origin(_url: &Url) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::UploadLink;
    use crate::data::Uri;

    const TOKEN: &str = "s3cr3t-download-token";

    #[test]
    fn accepts_an_upload_service_url() {
        let uri = UploadLink::parse("https://upload.elastic.co/d/abc123", TOKEN)
            .expect("valid link")
            .into_uri();
        let Uri::ServiceLink(url) = uri else {
            panic!("expected a service link");
        };
        assert_eq!(url.username(), "token");
        assert_eq!(url.password(), Some(TOKEN));
        assert_eq!(url.path(), "/d/abc123");
    }

    #[test]
    fn rejects_everything_else_without_repeating_the_token() {
        let env = crate::TestEnv::new();
        std::fs::write(
            &env.hosts_path,
            "prod:\n  app: elasticsearch\n  url: https://prod.example:9200\n",
        )
        .unwrap();
        let archive = env.tmp.path().join("diag.zip");
        std::fs::write(&archive, b"zip").unwrap();
        let archive = archive.display().to_string();
        let directory = env.tmp.path().display().to_string();
        let embedded = format!("https://token:{TOKEN}@upload.elastic.co/d/abc");
        let cases = [
            (archive.as_str(), TOKEN),
            (directory.as_str(), TOKEN),
            ("/tmp/diag.zip", TOKEN),
            ("C:\\diag.zip", TOKEN),
            ("file:///tmp/diag.zip", TOKEN),
            ("prod", TOKEN),
            ("https://example.com/d/abc", TOKEN),
            ("https://upload.elastic.co.example.com/d/abc", TOKEN),
            ("https://upload.elastic.co:8443/d/abc", TOKEN),
            ("http://upload.elastic.co/d/abc", TOKEN),
            ("https://upload.elastic.co/d/abc?x=1", TOKEN),
            ("https://upload.elastic.co/d/abc#frag", TOKEN),
            (embedded.as_str(), TOKEN),
            ("https://token@upload.elastic.co/d/abc", TOKEN),
            ("https://upload.elastic.co/", TOKEN),
            ("https://upload.elastic.co/d/abc", ""),
            ("https://upload.elastic.co/d/abc", "two words"),
        ];
        for (url, token) in cases {
            let error = UploadLink::parse(url, token)
                .err()
                .unwrap_or_else(|| panic!("{url} accepted"));
            assert!(!error.contains(TOKEN), "{url}: {error}");
        }
    }
}
