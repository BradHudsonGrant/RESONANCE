//! The one place requests leave the machine. Providers build requests; the client sends them.

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

/// Sent with every request. MusicBrainz asks for a way to reach the application's maintainers,
/// so it names the public repository.
pub const USER_AGENT: &str = concat!(
    "Resonance/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/BradHudsonGrant/RESONANCE )"
);

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    /// A form-encoded body, for requests too long for a URL such as a fingerprint lookup.
    PostForm,
}

#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    /// Query parameters, or the form body for `PostForm`.
    pub params: Vec<(String, String)>,
    /// A key sent alongside: as a form field for AcoustID, as a header for Discogs. It is never
    /// part of the cache key and never formatted.
    pub secret: Option<Secret>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Secret {
    FormField { name: &'static str, value: String },
    Header { name: &'static str, value: String },
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("params", &self.params)
            .field("secret", &self.secret.as_ref().map(|_| ".."))
            .finish()
    }
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            params: Vec::new(),
            secret: None,
        }
    }

    pub fn post_form(url: impl Into<String>) -> Self {
        Self {
            method: Method::PostForm,
            ..Self::get(url)
        }
    }

    pub fn param(mut self, name: &str, value: impl Into<String>) -> Self {
        self.params.push((name.to_owned(), value.into()));
        self
    }

    pub fn with_secret(mut self, secret: Secret) -> Self {
        self.secret = Some(secret);
        self
    }

    /// What identifies the request in the cache: everything but the secret.
    pub fn cache_key(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(match self.method {
            Method::Get => b"GET\n",
            Method::PostForm => b"POST\n",
        });
        hasher.update(self.url.as_bytes());
        for (name, value) in &self.params {
            hasher.update(b"\n");
            hasher.update(name.as_bytes());
            hasher.update(b"=");
            hasher.update(value.as_bytes());
        }
        *hasher.finalize().as_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// The request never got an answer: no network, a timeout or a TLS failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreachable(pub String);

impl fmt::Display for Unreachable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Sends a request. The real one goes over the network; tests answer from fixtures.
pub trait HttpClient: Send + Sync {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, Unreachable>;
}

/// HTTPS with the operating system's certificate store. The agent is made on the first request,
/// so starting the app costs nothing.
#[derive(Default)]
pub struct NetworkClient {
    agent: OnceLock<ureq::Agent>,
}

impl NetworkClient {
    pub fn new() -> Self {
        Self::default()
    }

    fn agent(&self) -> &ureq::Agent {
        self.agent.get_or_init(|| {
            let tls = ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build();
            ureq::Agent::config_builder()
                .timeout_global(Some(TIMEOUT))
                .http_status_as_error(false)
                .user_agent(USER_AGENT)
                .tls_config(tls)
                .build()
                .into()
        })
    }
}

impl HttpClient for NetworkClient {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
        let sent = match request.method {
            Method::Get => {
                let mut builder = self.agent().get(&request.url);
                for (name, value) in &request.params {
                    builder = builder.query(name, value);
                }
                if let Some(Secret::Header { name, value }) = &request.secret {
                    builder = builder.header(*name, value);
                }
                builder.call()
            }
            Method::PostForm => {
                let mut builder = self.agent().post(&request.url);
                if let Some(Secret::Header { name, value }) = &request.secret {
                    builder = builder.header(*name, value);
                }
                let mut form: Vec<(&str, &str)> = request
                    .params
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect();
                if let Some(Secret::FormField { name, value }) = &request.secret {
                    form.push((name, value));
                }
                builder.send_form(form)
            }
        };
        // ureq errors can quote the URL, and an AcoustID URL never carries the key, but the
        // message is kept to the kind of failure anyway.
        let mut response = sent.map_err(|error| Unreachable(failure_kind(&error)))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| Unreachable(failure_kind(&error)))?;
        Ok(HttpResponse { status, body })
    }
}

fn failure_kind(error: &ureq::Error) -> String {
    match error {
        ureq::Error::Timeout(_) => "The request timed out".to_owned(),
        ureq::Error::HostNotFound => {
            "The server could not be found. Check the connection".to_owned()
        }
        ureq::Error::ConnectionFailed => "The connection failed".to_owned(),
        ureq::Error::Io(_) => "The connection dropped".to_owned(),
        ureq::Error::BodyExceedsLimit(_) => "The answer was too large".to_owned(),
        _ => "The request failed".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secret_is_neither_hashed_nor_shown() {
        let plain = HttpRequest::get("https://example.test/a").param("q", "x");
        let keyed = plain.clone().with_secret(Secret::Header {
            name: "Authorization",
            value: "hidden-token".to_owned(),
        });
        assert_eq!(plain.cache_key(), keyed.cache_key());
        assert!(!format!("{keyed:?}").contains("hidden-token"));
        assert_ne!(
            plain.cache_key(),
            HttpRequest::get("https://example.test/a")
                .param("q", "y")
                .cache_key()
        );
    }

    #[test]
    fn the_user_agent_names_the_public_repository() {
        assert!(USER_AGENT.starts_with("Resonance/"));
        assert!(USER_AGENT.ends_with("( https://github.com/BradHudsonGrant/RESONANCE )"));
    }
}
