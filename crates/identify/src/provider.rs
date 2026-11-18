use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use resonance_core::identify::{Candidate, KnownTags, ReleaseUrl};
use resonance_core::providers::ProviderKey;
use resonance_db::{Database, DbError};

use crate::credentials::{CredentialError, ProviderSecret};
use crate::http::{HttpClient, HttpRequest};
use crate::rate::{RateLimit, RateLimiter};

/// Answers are kept this long, so a second run over the same files asks no one again.
pub const CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Debug)]
pub enum IdentifyError {
    Unreachable {
        provider: &'static str,
        reason: String,
    },
    Status {
        provider: &'static str,
        status: u16,
    },
    Refused {
        provider: &'static str,
        reason: &'static str,
    },
    Unexpected {
        provider: &'static str,
    },
    Database(DbError),
    Credentials(CredentialError),
}

impl fmt::Display for IdentifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable { provider, reason } => {
                write!(
                    f,
                    "{} could not be reached: {reason}",
                    display_name(provider)
                )
            }
            Self::Status { provider, status } => {
                write!(
                    f,
                    "{} answered with an error ({status})",
                    display_name(provider)
                )
            }
            Self::Refused { provider, reason } => write!(f, "{}: {reason}", display_name(provider)),
            Self::Unexpected { provider } => {
                write!(
                    f,
                    "{} sent an answer Resonance could not read",
                    display_name(provider)
                )
            }
            Self::Database(source) => source.fmt(f),
            Self::Credentials(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for IdentifyError {}

impl From<DbError> for IdentifyError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

/// How a provider id reads in a message.
pub fn display_name(provider: &str) -> &str {
    match provider {
        "acoustid" => "AcoustID",
        "musicbrainz" => "MusicBrainz",
        "discogs" => "Discogs",
        "bandcamp" => "Bandcamp",
        other => other,
    }
}

/// What a provider is asked.
#[derive(Debug, Clone, Copy)]
pub enum Query<'a> {
    /// A Chromaprint fingerprint and the full track's length in whole seconds.
    Fingerprint {
        fingerprint: &'a str,
        duration_s: u32,
    },
    /// The releases a MusicBrainz recording appears on, once a fingerprint found it.
    Recording {
        id: &'a str,
    },
    Text(&'a KnownTags),
    Release(&'a ReleaseUrl),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    Fingerprint,
    Recording,
    Text,
    Release,
}

impl Query<'_> {
    pub fn kind(&self) -> QueryKind {
        match self {
            Self::Fingerprint { .. } => QueryKind::Fingerprint,
            Self::Recording { .. } => QueryKind::Recording,
            Self::Text(_) => QueryKind::Text,
            Self::Release(_) => QueryKind::Release,
        }
    }
}

/// One metadata service. Each declares what it answers, the key it needs and its rate limit,
/// and the pipeline holds it to that limit.
pub trait MetadataProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn answers(&self, kind: QueryKind) -> bool;
    /// The key a kind of query cannot run without. A provider may still use an optional key.
    fn requires_key(&self, kind: QueryKind) -> Option<ProviderKey>;
    /// The key it uses when the user has entered one.
    fn key(&self) -> Option<ProviderKey>;
    fn rate_limit(&self) -> RateLimit;
    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError>;

    /// Several queries at once, where the provider takes them in one request.
    fn lookup_batch(
        &self,
        queries: &[Query<'_>],
        lookup: &Lookup<'_>,
    ) -> Vec<Result<Vec<Candidate>, IdentifyError>> {
        queries
            .iter()
            .map(|query| self.lookup(query, lookup))
            .collect()
    }
}

/// What a provider gets for one lookup: a paced, cached way to fetch, and its key if any.
pub struct Lookup<'a> {
    pub(crate) provider: &'static str,
    pub(crate) fetcher: &'a Fetcher,
    pub(crate) limiter: &'a RateLimiter,
    pub(crate) key: Option<&'a ProviderSecret>,
}

impl Lookup<'_> {
    pub fn key(&self) -> Option<&ProviderSecret> {
        self.key
    }

    /// The body of a successful answer, or `None` when the provider has no such thing.
    pub fn fetch(&self, request: &HttpRequest) -> Result<Option<String>, IdentifyError> {
        self.fetcher.fetch(self.provider, self.limiter, request)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FetchCounts {
    pub network: u32,
    pub cached: u32,
}

/// Fetches through the response cache, waiting on the provider's limiter only when it has to
/// ask the network.
pub struct Fetcher {
    http: Arc<dyn HttpClient>,
    database: Arc<Mutex<Database>>,
    counts: Mutex<FetchCounts>,
}

const NOT_FOUND: u16 = 404;
const OK: u16 = 200;

impl Fetcher {
    pub fn new(http: Arc<dyn HttpClient>, database: Arc<Mutex<Database>>) -> Self {
        Self {
            http,
            database,
            counts: Mutex::new(FetchCounts::default()),
        }
    }

    pub fn counts(&self) -> FetchCounts {
        *self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn fetch(
        &self,
        provider: &'static str,
        limiter: &RateLimiter,
        request: &HttpRequest,
    ) -> Result<Option<String>, IdentifyError> {
        let key = request.cache_key();
        let fresh_since = now_ns().saturating_sub(CACHE_TTL.as_nanos() as i64);
        let cached = self
            .database
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .cached_response(provider, &key, fresh_since)?;
        if let Some(cached) = cached {
            self.counts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .cached += 1;
            return Ok((cached.status == OK).then_some(cached.body));
        }

        let mut response = None;
        // One retry after a "slow down": MusicBrainz answers 503 and Discogs 429 when a burst
        // from another program on the same address used the allowance.
        for _ in 0..2 {
            limiter.acquire();
            self.counts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .network += 1;
            let answer = self
                .http
                .send(request)
                .map_err(|failure| IdentifyError::Unreachable {
                    provider,
                    reason: failure.0,
                })?;
            let busy = matches!(answer.status, 429 | 503);
            response = Some(answer);
            if !busy {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        let Some(response) = response else {
            return Err(IdentifyError::Unexpected { provider });
        };
        match response.status {
            OK | NOT_FOUND => {
                {
                    let mut database = self.database.lock().unwrap_or_else(PoisonError::into_inner);
                    database.store_response(provider, &key, response.status, &response.body)?;
                }
                Ok((response.status == OK).then_some(response.body))
            }
            status => Err(IdentifyError::Status { provider, status }),
        }
    }
}

pub(crate) fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as i64)
        .unwrap_or(0)
}

/// Reads JSON a provider sent.
pub(crate) fn parse_json(
    provider: &'static str,
    body: &str,
) -> Result<serde_json::Value, IdentifyError> {
    serde_json::from_str(body).map_err(|_| IdentifyError::Unexpected { provider })
}

/// A JSON string field, trimmed, or `None` when missing or blank.
pub(crate) fn text(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The year at the start of a date such as `2019-05-03`, or anywhere in `03 May 2019`.
pub(crate) fn year_in(date: &str) -> Option<u16> {
    date.split(|c: char| !c.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .find_map(|part| part.parse::<u16>().ok())
        .filter(|year| (1900..=2100).contains(year))
}
