use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use resonance_core::artwork::MAX_ARTWORK_BYTES;
use resonance_core::identify::{Candidate, KnownTags, ReleaseUrl};
use resonance_core::providers::ProviderKey;
use resonance_db::{Database, DbError};

use crate::credentials::{CredentialError, ProviderSecret};
use crate::http::{HttpClient, HttpRequest};
use crate::rate::{RateLimit, RateLimiter};

/// Answers are kept this long, so a second run over the same files asks no one again.
pub const CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Maximum size for an in-memory 30-second audio preview stream (2 MiB).
pub const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;

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

/// How a provider id reads in a message or UI label.
pub fn display_name(provider: &str) -> &str {
    match provider {
        "acoustid" => "AcoustID",
        "musicbrainz" => "MusicBrainz",
        "discogs" => "Discogs",
        "bandcamp" => "Bandcamp",
        "deezer" => "Deezer",
        "itunes" => "Apple Music",
        "soundcloud" => "SoundCloud",
        "youtube" => "YouTube",
        "audd" => "AudD",
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
    /// An ISO 3901 ISRC (12 uppercase alphanumeric chars, no hyphens).
    Isrc(&'a str),
    /// A web page or stream URL checked against MusicBrainz URL relationships.
    Url(&'a str),
    /// A compact WAV excerpt kept in RAM for opt-in recognisers.
    AudioExcerpt {
        wav_bytes: &'a [u8],
        audio_hash_hex: &'a str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    Fingerprint,
    Recording,
    Text,
    Release,
    Isrc,
    Url,
    AudioExcerpt,
}

impl Query<'_> {
    pub fn kind(&self) -> QueryKind {
        match self {
            Self::Fingerprint { .. } => QueryKind::Fingerprint,
            Self::Recording { .. } => QueryKind::Recording,
            Self::Text(_) => QueryKind::Text,
            Self::Release(_) => QueryKind::Release,
            Self::Isrc(_) => QueryKind::Isrc,
            Self::Url(_) => QueryKind::Url,
            Self::AudioExcerpt { .. } => QueryKind::AudioExcerpt,
        }
    }
}

/// How the shared fetcher should treat one HTTP answer without hardcoding provider quirks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseDisposition {
    /// Store status + body in `provider_cache`. `200` returns `Some(body)`, `404` returns `None`.
    Cacheable,
    /// Return `Some(body)` to the provider without caching (so it can report typed refusals).
    PassThrough,
    /// Transient quota/server pressure: wait `Retry-After` (or 1 s) and try once more. Never cached.
    Retryable,
    /// Hard HTTP or API error; fails the lookup cleanly without caching.
    Error,
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

    /// How long a cached answer stays good. An answer that carries signed links has to be
    /// asked for again before they expire.
    fn cache_ttl(&self) -> Duration {
        CACHE_TTL
    }

    fn classify_response(&self, status: u16, _body: &str) -> ResponseDisposition {
        match status {
            OK | NOT_FOUND => ResponseDisposition::Cacheable,
            429 | 503 => ResponseDisposition::Retryable,
            _ => ResponseDisposition::Error,
        }
    }

    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError>;
}

/// What a provider gets for one lookup: a paced, cached way to fetch, and its key if any.
pub struct Lookup<'a> {
    pub(crate) provider: &'a dyn MetadataProvider,
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
        source: &dyn MetadataProvider,
        limiter: &RateLimiter,
        request: &HttpRequest,
    ) -> Result<Option<String>, IdentifyError> {
        let provider = source.id();
        let key = request.cache_key();
        let fresh_since = now_ns().saturating_sub(source.cache_ttl().as_nanos() as i64);
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
        let mut disposition = ResponseDisposition::Error;
        // One retry after a "slow down": MusicBrainz answers 503 and Discogs 429 when a burst
        // from another program on the same address used the allowance.
        for attempt in 0..2 {
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
            disposition = source.classify_response(answer.status, &answer.body);
            // The HTTP client has already clamped Retry-After to a few seconds.
            let wait_s = answer.retry_after_s.unwrap_or(1);
            response = Some(answer);
            if disposition != ResponseDisposition::Retryable || attempt == 1 {
                break;
            }
            std::thread::sleep(Duration::from_secs(wait_s));
        }
        let Some(response) = response else {
            return Err(IdentifyError::Unexpected { provider });
        };

        match disposition {
            ResponseDisposition::Cacheable => {
                {
                    let mut database = self.database.lock().unwrap_or_else(PoisonError::into_inner);
                    database.store_response(provider, &key, response.status, &response.body)?;
                }
                Ok((response.status == OK).then_some(response.body))
            }
            ResponseDisposition::PassThrough => Ok(Some(response.body)),
            ResponseDisposition::Retryable | ResponseDisposition::Error => {
                Err(IdentifyError::Status {
                    provider,
                    status: response.status,
                })
            }
        }
    }

    /// A picture, fetched straight from the network after waiting on the limiter. Pictures are
    /// kept in the picture store, not the response cache. `None` when the address has none.
    pub(crate) fn fetch_picture(
        &self,
        provider: &'static str,
        limiter: &RateLimiter,
        url: &str,
    ) -> Result<Option<Vec<u8>>, IdentifyError> {
        limiter.acquire();
        self.counts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .network += 1;
        let answer = self
            .http
            .fetch_bytes(&HttpRequest::get(url), MAX_ARTWORK_BYTES as u64)
            .map_err(|failure| IdentifyError::Unreachable {
                provider,
                reason: failure.0,
            })?;
        match answer.status {
            OK => Ok(Some(answer.body)),
            NOT_FOUND => Ok(None),
            status => Err(IdentifyError::Status { provider, status }),
        }
    }

    /// Fetches a 30-second audio preview strictly into RAM (never cached, never written to disk).
    pub(crate) fn fetch_preview(
        &self,
        provider: &'static str,
        limiter: &RateLimiter,
        url: &str,
    ) -> Result<Option<Vec<u8>>, IdentifyError> {
        if !url.starts_with("https://") {
            return Ok(None);
        }
        limiter.acquire();
        self.counts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .network += 1;
        let answer = self
            .http
            .fetch_bytes(&HttpRequest::get(url), MAX_PREVIEW_BYTES)
            .map_err(|failure| IdentifyError::Unreachable {
                provider,
                reason: failure.0,
            })?;
        match answer.status {
            OK => Ok(Some(answer.body)),
            NOT_FOUND => Ok(None),
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
