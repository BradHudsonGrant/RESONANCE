//! The six-step identification cascade, multi-provider fusion, ISRC bridging, in-RAM
//! 30-second preview Chromaprint confirmation and opt-in audio recognition.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use resonance_core::identify::{
    Candidate, ENRICHMENT_THRESHOLD, FINGERPRINT_MATCH_SCORE, KnownTags, MAX_STORED_CANDIDATES,
    RankedCandidate, ReleaseUrl, STOP_THRESHOLD, Step, StepOutcome, confirm_by_fingerprint,
    fuse_and_rank, guessed_bandcamp_pages, is_same_candidate, is_viable_alternative,
    needs_preview_check, normalise_tags, parse_path, replaces_best, retain_viable_candidates,
};
use resonance_core::providers::ProviderKey;
use resonance_db::{Database, IdentificationInput};

use crate::audio::AudioFingerprinter;
use crate::credentials::{CredentialError, CredentialStore, ProviderSecret};
use crate::http::HttpClient;
use crate::provider::{
    FetchCounts, Fetcher, IdentifyError, Lookup, MetadataProvider, Query, QueryKind,
};
use crate::providers::{
    AcoustId, AudD, Bandcamp, Deezer, Discogs, Itunes, MusicBrainz, SoundCloud, YouTube,
};
use crate::rate::{RateLimit, RateLimiter};

/// How many fingerprint matches are followed to their releases on MusicBrainz.
const RECORDINGS_FOLLOWED: usize = 2;

/// What a step did for a track, and which provider did it.
#[derive(Debug, Clone, PartialEq)]
pub struct StepReport {
    pub step: Step,
    pub provider: Option<&'static str>,
    pub outcome: StepOutcome,
}

/// The answer for one track, keeping up to `MAX_STORED_CANDIDATES` ranked options.
#[derive(Debug, Clone, PartialEq)]
pub struct Identification {
    pub track_id: i64,
    pub known: KnownTags,
    pub best: Option<RankedCandidate>,
    pub candidates: Vec<RankedCandidate>,
    pub steps: Vec<StepReport>,
}

impl Identification {
    pub fn is_confident(&self) -> bool {
        self.best
            .as_ref()
            .is_some_and(|best| best.confidence >= STOP_THRESHOLD)
    }

    /// Every applicable free step ran cleanly. Unconfigured paid recognisers never hold back
    /// completion.
    pub fn is_complete(&self) -> bool {
        !self.steps.iter().any(|report| {
            matches!(report.outcome, StepOutcome::Failed(_))
                || matches!(
                    report.outcome,
                    StepOutcome::SkippedMissingKey(key) if !key.is_paid()
                )
        })
    }

    pub fn needs_url(&self) -> bool {
        self.steps
            .iter()
            .any(|report| report.outcome == StepOutcome::NeedsUrl)
    }
}

/// Steps 1 and 2, which need no network.
pub fn identify_locally(input: &IdentificationInput) -> Identification {
    let from_tags = normalise_tags(&input.tags);
    let from_path = parse_path(&input.path);
    let mut known = from_tags.clone();
    known.fill_from(&from_path);
    if known.isrc.is_none()
        && let Some(file_isrc) = &input.file_isrc
    {
        known.isrc = Some(file_isrc.clone());
    }
    Identification {
        track_id: input.track_id,
        known,
        best: None,
        candidates: Vec::new(),
        steps: vec![
            StepReport {
                step: Step::ExistingTags,
                provider: None,
                outcome: StepOutcome::Read {
                    fields: from_tags.field_count(),
                },
            },
            StepReport {
                step: Step::FileName,
                provider: None,
                outcome: StepOutcome::Read {
                    fields: from_path.field_count(),
                },
            },
        ],
    }
}

pub(crate) struct Registered {
    pub(crate) provider: Box<dyn MetadataProvider>,
    pub(crate) limiter: RateLimiter,
    pub(crate) artwork_limiter: RateLimiter,
}

/// The providers, each with its own limiter, the cache they share, and an optional audio
/// decoder for step 5 in-memory preview confirmation and opt-in recognisers.
pub struct Pipeline {
    pub(crate) registered: Vec<Registered>,
    pub(crate) fetcher: Fetcher,
    pub(crate) database: Arc<Mutex<Database>>,
    credentials: Arc<dyn CredentialStore>,
    pub(crate) fingerprinter: Option<Arc<dyn AudioFingerprinter>>,
    verify_previews: bool,
    stop_threshold: f64,
}

pub(crate) struct TrackState {
    pub(crate) identification: Identification,
    pub(crate) duration_ms: Option<u64>,
    fingerprint: Option<(String, u32)>,
    fingerprinted: Vec<Candidate>,
    pub(crate) collected: Vec<Candidate>,
    queried_text_providers: Vec<&'static str>,
    pub(crate) done: bool,
    confident_from_fingerprint: bool,
}

pub(crate) type Keys = HashMap<ProviderKey, Result<Option<ProviderSecret>, CredentialError>>;

impl Pipeline {
    pub fn new(
        providers: Vec<Box<dyn MetadataProvider>>,
        http: Arc<dyn HttpClient>,
        database: Arc<Mutex<Database>>,
        credentials: Arc<dyn CredentialStore>,
    ) -> Self {
        Self {
            registered: providers
                .into_iter()
                .map(|provider| Registered {
                    limiter: RateLimiter::new(provider.rate_limit()),
                    artwork_limiter: RateLimiter::new(RateLimit::per_second(4)),
                    provider,
                })
                .collect(),
            fetcher: Fetcher::new(http, Arc::clone(&database)),
            database,
            credentials,
            fingerprinter: None,
            verify_previews: true,
            stop_threshold: STOP_THRESHOLD,
        }
    }

    /// Registers all built-in metadata providers and opt-in recognisers (which stay inactive
    /// until their key is stored in Settings).
    pub fn with_free_providers(
        http: Arc<dyn HttpClient>,
        database: Arc<Mutex<Database>>,
        credentials: Arc<dyn CredentialStore>,
    ) -> Self {
        Self::new(
            vec![
                Box::new(AcoustId),
                Box::new(Deezer),
                Box::new(MusicBrainz),
                Box::new(Discogs),
                Box::new(Itunes),
                Box::new(Bandcamp),
                Box::new(SoundCloud),
                Box::new(YouTube),
                Box::new(AudD),
            ],
            http,
            database,
            credentials,
        )
    }

    pub fn with_fingerprinter(mut self, fingerprinter: Arc<dyn AudioFingerprinter>) -> Self {
        self.fingerprinter = Some(fingerprinter);
        self
    }

    pub fn with_verify_previews(mut self, verify: bool) -> Self {
        self.verify_previews = verify;
        self
    }

    /// The picture at `url`, using the provider's dedicated `artwork_limiter`.
    pub fn fetch_artwork(
        &self,
        provider: &str,
        url: &str,
    ) -> Result<Option<Vec<u8>>, IdentifyError> {
        let Some(registered) = self
            .registered
            .iter()
            .find(|registered| registered.provider.id() == provider)
        else {
            return Ok(None);
        };
        self.fetcher
            .fetch_picture(registered.provider.id(), &registered.artwork_limiter, url)
    }

    /// How many answers came from the network and how many from the cache so far.
    pub fn fetch_counts(&self) -> FetchCounts {
        self.fetcher.counts()
    }

    fn keys(&self) -> Keys {
        let mut keys = Keys::new();
        for registered in &self.registered {
            if let Some(key) = registered.provider.key() {
                keys.entry(key)
                    .or_insert_with(|| self.credentials.read(key));
            }
        }
        keys
    }

    pub(crate) fn find_registered(&self, id: &str) -> Option<&Registered> {
        self.registered.iter().find(|r| r.provider.id() == id)
    }

    pub(crate) fn lookup<'a>(&'a self, registered: &'a Registered, keys: &'a Keys) -> Lookup<'a> {
        Lookup {
            provider: registered.provider.as_ref(),
            fetcher: &self.fetcher,
            limiter: &registered.limiter,
            key: registered
                .provider
                .key()
                .and_then(|key| keys.get(&key))
                .and_then(|read| read.as_ref().ok())
                .and_then(Option::as_ref),
        }
    }

    /// Why a provider cannot take a kind of query, if it cannot.
    pub(crate) fn blocked(
        &self,
        registered: &Registered,
        kind: QueryKind,
        keys: &Keys,
    ) -> Option<StepOutcome> {
        let needed = registered.provider.requires_key(kind)?;
        match keys.get(&needed) {
            Some(Ok(Some(_))) => None,
            Some(Err(error)) => Some(StepOutcome::Failed(error.to_string())),
            _ => Some(StepOutcome::SkippedMissingKey(needed)),
        }
    }

    pub(crate) fn answering(&self, kind: QueryKind) -> impl Iterator<Item = &Registered> {
        self.registered
            .iter()
            .filter(move |registered| registered.provider.answers(kind))
    }

    /// Runs steps 1 to 6 for each input. Stops cleanly when `abort` is set.
    pub fn identify(
        &self,
        inputs: &[IdentificationInput],
        abort: &AtomicBool,
    ) -> Vec<Identification> {
        let keys = self.keys();
        let mut states: Vec<TrackState> = inputs
            .iter()
            .map(|input| TrackState {
                identification: {
                    let mut local = identify_locally(input);
                    local
                        .known
                        .bandcamp_sellers
                        .clone_from(&input.bandcamp_sellers);
                    local
                },
                duration_ms: input.duration_ms,
                fingerprint: input
                    .fingerprint
                    .clone()
                    .zip(input.fingerprint_duration_s.filter(|seconds| *seconds > 0)),
                fingerprinted: Vec::new(),
                collected: Vec::new(),
                queried_text_providers: Vec::new(),
                done: false,
                confident_from_fingerprint: false,
            })
            .collect();

        self.embedded_url_step(&mut states, &keys, abort);
        self.isrc_fast_step(&mut states, &keys, abort);
        self.fingerprint_step(&mut states, &keys, abort);
        self.text_step(&mut states, &keys, abort);
        self.bandcamp_page_step(&mut states, &keys, abort);
        self.enrichment_bridge_step(&mut states, &keys, abort);

        for state in &mut states {
            if abort.load(Ordering::Acquire) {
                break;
            }
            self.confirmation_step(state, &keys, abort);
            let outcome = if state.done {
                StepOutcome::SkippedConfident
            } else {
                StepOutcome::NeedsUrl
            };
            report(state, Step::UserUrl, None, outcome);
        }

        states
            .into_iter()
            .map(|state| state.identification)
            .collect()
    }

    fn embedded_url_step(&self, states: &mut [TrackState], keys: &Keys, abort: &AtomicBool) {
        for state in states.iter_mut() {
            if abort.load(Ordering::Acquire) {
                return;
            }
            let urls = state.identification.known.embedded_urls.clone();
            if urls.is_empty() {
                continue;
            }
            for url in &urls {
                let mut found = self.resolve_release_url(url, false, true, keys);
                if found.is_empty() {
                    continue;
                }
                state.collected.append(&mut found);
            }
            self.settle(state);
        }
    }

    fn isrc_fast_step(&self, states: &mut [TrackState], keys: &Keys, abort: &AtomicBool) {
        for state in states.iter_mut() {
            if abort.load(Ordering::Acquire) {
                return;
            }
            let Some(isrc) = state.identification.known.isrc.clone() else {
                continue;
            };
            for registered in self.answering(QueryKind::Isrc) {
                if self.blocked(registered, QueryKind::Isrc, keys).is_some() {
                    continue;
                }
                let lookup = self.lookup(registered, keys);
                if let Ok(found) = registered.provider.lookup(&Query::Isrc(&isrc), &lookup) {
                    state.collected.extend(found);
                }
            }
            self.settle(state);
        }
    }

    fn fingerprint_step(&self, states: &mut [TrackState], keys: &Keys, abort: &AtomicBool) {
        for registered in self.answering(QueryKind::Fingerprint) {
            let id = registered.provider.id();
            if let Some(outcome) = self.blocked(registered, QueryKind::Fingerprint, keys) {
                for state in states.iter_mut() {
                    if state.done {
                        report(
                            state,
                            Step::Fingerprint,
                            Some(id),
                            StepOutcome::SkippedConfident,
                        );
                    } else if state.fingerprint.is_none() {
                        report(
                            state,
                            Step::Fingerprint,
                            Some(id),
                            StepOutcome::SkippedNoInput,
                        );
                    } else {
                        report(state, Step::Fingerprint, Some(id), outcome.clone());
                    }
                }
                continue;
            }
            let mut asked = Vec::new();
            for (index, state) in states.iter_mut().enumerate() {
                if state.done {
                    report(
                        state,
                        Step::Fingerprint,
                        Some(id),
                        StepOutcome::SkippedConfident,
                    );
                } else if state.fingerprint.is_none() {
                    report(
                        state,
                        Step::Fingerprint,
                        Some(id),
                        StepOutcome::SkippedNoInput,
                    );
                } else {
                    asked.push(index);
                }
            }
            if asked.is_empty() || abort.load(Ordering::Acquire) {
                continue;
            }
            // AcoustID takes several fingerprints in one request, so the whole batch goes at once.
            let answers = {
                let queries: Vec<Query<'_>> = asked
                    .iter()
                    .filter_map(|&index| states[index].fingerprint.as_ref())
                    .map(|(fingerprint, duration_s)| Query::Fingerprint {
                        fingerprint,
                        duration_s: *duration_s,
                    })
                    .collect();
                let lookup = self.lookup(registered, keys);
                registered.provider.lookup_batch(&queries, &lookup)
            };
            for (index, answer) in asked.into_iter().zip(answers) {
                if abort.load(Ordering::Acquire) {
                    return;
                }
                let state = &mut states[index];
                match answer {
                    Ok(found) => {
                        let count = found.len();
                        state.fingerprinted.extend(found);
                        self.follow_recordings(state, keys);
                        let best = self.settle(state);
                        if state.done {
                            state.confident_from_fingerprint = true;
                        }
                        report(state, Step::Fingerprint, Some(id), ran(count, best));
                    }
                    Err(error) => report(
                        state,
                        Step::Fingerprint,
                        Some(id),
                        StepOutcome::Failed(error.to_string()),
                    ),
                }
            }
        }
    }
    // A fingerprint match names a recording; its releases carry the label, catalogue number
    // and year.
    fn follow_recordings(&self, state: &mut TrackState, keys: &Keys) {
        let mut followed: Vec<Candidate> = Vec::new();
        let mut matches: Vec<&Candidate> = state
            .fingerprinted
            .iter()
            .filter(|candidate| {
                candidate
                    .fingerprint_score
                    .is_some_and(|score| score >= FINGERPRINT_MATCH_SCORE)
            })
            .collect();
        matches.sort_by(|a, b| {
            let score = |candidate: &Candidate| candidate.fingerprint_score.unwrap_or(0.0);
            score(b).total_cmp(&score(a))
        });
        matches.dedup_by(|a, b| a.recording_id == b.recording_id);
        for registered in self.answering(QueryKind::Recording) {
            let id = registered.provider.id();
            if self
                .blocked(registered, QueryKind::Recording, keys)
                .is_some()
            {
                continue;
            }
            let lookup = self.lookup(registered, keys);
            for matched in matches.iter().take(RECORDINGS_FOLLOWED) {
                let Some(recording) = matched.recording_id.as_deref() else {
                    continue;
                };
                match registered
                    .provider
                    .lookup(&Query::Recording { id: recording }, &lookup)
                {
                    Ok(releases) => {
                        followed.extend(releases.into_iter().map(|release| Candidate {
                            provider: matched.provider.clone(),
                            artist: release.artist.or_else(|| matched.artist.clone()),
                            title: release.title.or_else(|| matched.title.clone()),
                            mix: release.mix.or_else(|| matched.mix.clone()),
                            album: release.album.or_else(|| matched.album.clone()),
                            duration_ms: release.duration_ms.or(matched.duration_ms),
                            fingerprint_score: matched.fingerprint_score,
                            audio_source: matched.audio_source,
                            recording_id: matched.recording_id.clone(),
                            ..release
                        }))
                    }
                    Err(error) => {
                        state.identification.steps.push(StepReport {
                            step: Step::Fingerprint,
                            provider: Some(id),
                            outcome: StepOutcome::Failed(error.to_string()),
                        });
                    }
                }
            }
        }
        // A recording found on its releases is replaced by them: they say the same and more.
        let enriched: Vec<Option<String>> = followed
            .iter()
            .map(|candidate| candidate.recording_id.clone())
            .collect();
        state
            .fingerprinted
            .retain(|candidate| !enriched.contains(&candidate.recording_id));
        state.fingerprinted.extend(followed);
    }

    fn text_step(&self, states: &mut [TrackState], keys: &Keys, abort: &AtomicBool) {
        for registered in self.answering(QueryKind::Text) {
            let id = registered.provider.id();
            let blocked = self.blocked(registered, QueryKind::Text, keys);
            let lookup = self.lookup(registered, keys);

            for state in states.iter_mut() {
                if state.done {
                    report(
                        state,
                        Step::TextSearch,
                        Some(id),
                        StepOutcome::SkippedConfident,
                    );
                    continue;
                }
                if let Some(outcome) = &blocked {
                    report(state, Step::TextSearch, Some(id), outcome.clone());
                    continue;
                }
                if !state.identification.known.is_searchable() {
                    report(
                        state,
                        Step::TextSearch,
                        Some(id),
                        StepOutcome::SkippedNoInput,
                    );
                    continue;
                }
                if abort.load(Ordering::Acquire) {
                    return;
                }
                let known = state.identification.known.clone();
                state.queried_text_providers.push(id);
                match registered.provider.lookup(&Query::Text(&known), &lookup) {
                    Ok(found) => {
                        let count = found.len();
                        state.collected.extend(found);
                        let best = self.settle(state);
                        report(state, Step::TextSearch, Some(id), ran(count, best));
                    }
                    Err(error) => report(
                        state,
                        Step::TextSearch,
                        Some(id),
                        StepOutcome::Failed(error.to_string()),
                    ),
                }
            }
        }
    }

    /// Edits and bootlegs that only Bandcamp sells are in no catalogue the text search reaches,
    /// so a track still unmatched reads the Bandcamp pages its names point at. A page that does
    /// not exist costs one request and is cached.
    fn bandcamp_page_step(&self, states: &mut [TrackState], keys: &Keys, abort: &AtomicBool) {
        let Some(bandcamp) = self.find_registered("bandcamp") else {
            return;
        };
        if self.blocked(bandcamp, QueryKind::Release, keys).is_some() {
            return;
        }
        let lookup = self.lookup(bandcamp, keys);
        for state in states.iter_mut() {
            if state.done {
                continue;
            }
            let pages = guessed_bandcamp_pages(&state.identification.known);
            if pages.is_empty() {
                continue;
            }
            let mut found = 0;
            let mut failure = None;
            for page in &pages {
                if abort.load(Ordering::Acquire) {
                    return;
                }
                match bandcamp.provider.lookup(&Query::Release(page), &lookup) {
                    Ok(candidates) if candidates.is_empty() => {}
                    Ok(candidates) => {
                        found += candidates.len();
                        state.collected.extend(candidates);
                        break;
                    }
                    // A guessed page that is not there is a 404 and comes back empty. Anything
                    // else, such as a dropped connection, leaves the track to be tried again.
                    Err(error) => {
                        failure = Some(error.to_string());
                        break;
                    }
                }
            }
            if found > 0 {
                let best = self.settle(state);
                report(state, Step::TextSearch, Some("bandcamp"), ran(found, best));
            } else if let Some(reason) = failure {
                report(
                    state,
                    Step::TextSearch,
                    Some("bandcamp"),
                    StepOutcome::Failed(reason),
                );
            } else {
                report(state, Step::TextSearch, Some("bandcamp"), ran(0, None));
            }
        }
    }

    fn enrichment_bridge_step(&self, states: &mut [TrackState], keys: &Keys, abort: &AtomicBool) {
        for state in states.iter_mut() {
            if abort.load(Ordering::Acquire) {
                return;
            }
            if state.confident_from_fingerprint {
                continue;
            }
            let Some(best) = state.identification.best.clone() else {
                continue;
            };
            if best.confidence < ENRICHMENT_THRESHOLD {
                continue;
            }

            // A store match carries an ISRC. MusicBrainz knows the same recording by it and has a
            // cover from the Cover Art Archive, which the store's terms do not allow us to embed.
            if let Some(isrc) = best.candidate.isrc.clone()
                && !state.collected.iter().any(|c| c.provider == "musicbrainz")
                && let Some(mb) = self.find_registered("musicbrainz")
                && self.blocked(mb, QueryKind::Isrc, keys).is_none()
            {
                let lookup = self.lookup(mb, keys);
                match mb.provider.lookup(&Query::Isrc(&isrc), &lookup) {
                    Ok(found) => {
                        let count = found.len();
                        if !found.is_empty() {
                            state.collected.extend(found);
                            let _ = self.settle(state);
                        }
                        let best_conf = state.identification.best.as_ref().map(|b| b.confidence);
                        upsert_step_report(
                            state,
                            Step::TextSearch,
                            "musicbrainz",
                            ran(count, best_conf),
                        );
                    }
                    Err(error) => upsert_step_report(
                        state,
                        Step::TextSearch,
                        "musicbrainz",
                        StepOutcome::Failed(error.to_string()),
                    ),
                }
            }

            // Stores rarely give a catalogue number or a cover we may embed. Discogs usually has
            // both, so a confident match that lacks them asks Discogs once.
            let missing_club_meta = best.candidate.catno.is_none()
                || best.candidate.artwork_fetcher_provider().is_none();
            if best.confidence >= self.stop_threshold
                && missing_club_meta
                && !state.queried_text_providers.contains(&"discogs")
                && let Some(discogs) = self.find_registered("discogs")
                && self.blocked(discogs, QueryKind::Text, keys).is_none()
            {
                let mut enriched_known = state.identification.known.clone();
                if enriched_known.artist.is_none() {
                    enriched_known.artist.clone_from(&best.candidate.artist);
                }
                if enriched_known.title.is_none() {
                    enriched_known.title.clone_from(&best.candidate.title);
                }
                if enriched_known.mix.is_none() {
                    enriched_known.mix.clone_from(&best.candidate.mix);
                }
                if enriched_known.is_searchable() {
                    let lookup = self.lookup(discogs, keys);
                    match discogs
                        .provider
                        .lookup(&Query::Text(&enriched_known), &lookup)
                    {
                        Ok(found) => {
                            let count = found.len();
                            if !found.is_empty() {
                                state.collected.extend(found);
                                let _ = self.settle(state);
                            }
                            let best_conf =
                                state.identification.best.as_ref().map(|b| b.confidence);
                            upsert_step_report(
                                state,
                                Step::TextSearch,
                                "discogs",
                                ran(count, best_conf),
                            );
                        }
                        Err(error) => upsert_step_report(
                            state,
                            Step::TextSearch,
                            "discogs",
                            StepOutcome::Failed(error.to_string()),
                        ),
                    }
                }
            }
        }
    }

    // Comparing fingerprints needs no network, so it runs even when text search was already
    // confident: that is how a result found by name learns that the audio matches too.
    fn confirmation_step(&self, state: &mut TrackState, keys: &Keys, abort: &AtomicBool) {
        if state.confident_from_fingerprint {
            report(
                state,
                Step::AudioConfirmation,
                None,
                StepOutcome::SkippedConfident,
            );
            return;
        }

        confirm_by_fingerprint(&mut state.collected, &state.fingerprinted);
        let fp_confirmed = state
            .collected
            .iter()
            .filter(|c| {
                c.fingerprint_score
                    .is_some_and(|s| s >= FINGERPRINT_MATCH_SCORE)
            })
            .count();

        let mut preview_confirmed = None;
        if self.verify_previews
            && self.fingerprinter.is_some()
            && !state.collected.is_empty()
            && !state
                .identification
                .best
                .as_ref()
                .is_some_and(|b| b.audio_matched)
            && needs_preview_check(state.identification.best.as_ref(), self.stop_threshold)
        {
            preview_confirmed = self.verify_audio_previews(state, keys, abort);
            // A preview match can settle the track, and then the paid recogniser is not asked.
            self.settle(state);
        }

        if !abort.load(Ordering::Acquire) {
            self.audd_fallback_step(state, keys);
        }

        let best = self.settle(state);
        let total_confirmed = fp_confirmed + preview_confirmed.unwrap_or(0);
        if total_confirmed == 0 && state.fingerprinted.is_empty() && preview_confirmed.is_none() {
            report(
                state,
                Step::AudioConfirmation,
                None,
                StepOutcome::SkippedNoInput,
            );
        } else {
            report(
                state,
                Step::AudioConfirmation,
                None,
                ran(total_confirmed, best),
            );
        }
    }

    pub(crate) fn settle(&self, state: &mut TrackState) -> Option<f64> {
        let mut everything = state.fingerprinted.clone();
        everything.extend(state.collected.clone());
        let ranked = retain_viable_candidates(fuse_and_rank(
            &state.identification.known,
            state.duration_ms,
            everything,
        ));
        let top: Vec<RankedCandidate> = ranked.into_iter().take(MAX_STORED_CANDIDATES).collect();
        let best = top.first()?.clone();
        let confidence = best.confidence;
        let replace = state
            .identification
            .best
            .as_ref()
            .is_none_or(|current| replaces_best(current, &best));
        if replace {
            state.identification.best = Some(best);
            state.identification.candidates = top;
        } else if let Some(current) = state.identification.best.clone() {
            let mut merged_top = vec![current.clone()];
            for alt in top {
                let is_current = is_same_candidate(&alt.candidate, &current.candidate);
                if !is_current && is_viable_alternative(&current, &alt) {
                    merged_top.push(alt);
                    if merged_top.len() >= MAX_STORED_CANDIDATES {
                        break;
                    }
                }
            }
            state.identification.candidates = merged_top;
        }
        if confidence >= self.stop_threshold {
            state.done = true;
        }
        Some(
            state
                .identification
                .best
                .as_ref()
                .map_or(confidence, |b| b.confidence),
        )
    }

    fn resolve_release_url(
        &self,
        url: &ReleaseUrl,
        user_supplied: bool,
        from_embedded_url: bool,
        keys: &Keys,
    ) -> Vec<Candidate> {
        let mut collected = Vec::new();
        for registered in self.answering(QueryKind::Release) {
            if self.blocked(registered, QueryKind::Release, keys).is_some() {
                continue;
            }
            let lookup = self.lookup(registered, keys);
            if let Ok(found) = registered.provider.lookup(&Query::Release(url), &lookup) {
                collected.extend(found.into_iter().map(|c| Candidate {
                    user_supplied,
                    from_embedded_url,
                    ..c
                }));
            }
        }
        collected.extend(
            self.linked_releases(url, keys)
                .into_iter()
                .map(|c| Candidate {
                    user_supplied,
                    from_embedded_url,
                    ..c
                }),
        );
        collected
    }

    /// Releases MusicBrainz links to a store page, so a pasted Bandcamp, SoundCloud or YouTube
    /// link can also bring the catalogue's release and its cover.
    fn linked_releases(&self, url: &ReleaseUrl, keys: &Keys) -> Vec<Candidate> {
        let Some(raw_url) = raw_page_url(url) else {
            return Vec::new();
        };
        let Some(mb) = self.find_registered("musicbrainz") else {
            return Vec::new();
        };
        if self.blocked(mb, QueryKind::Url, keys).is_some() {
            return Vec::new();
        }
        let lookup = self.lookup(mb, keys);
        mb.provider
            .lookup(&Query::Url(&raw_url), &lookup)
            .unwrap_or_default()
    }

    /// Step 6 for one track: the release page the user pasted, read as a confident source
    /// unless its length or mix plainly differs from the file.
    pub fn identify_with_url(
        &self,
        input: &IdentificationInput,
        url: &ReleaseUrl,
    ) -> Identification {
        let keys = self.keys();
        let mut state = TrackState {
            identification: identify_locally(input),
            duration_ms: input.duration_ms,
            fingerprint: None,
            fingerprinted: Vec::new(),
            collected: Vec::new(),
            queried_text_providers: Vec::new(),
            done: false,
            confident_from_fingerprint: false,
        };
        for registered in self.answering(QueryKind::Release) {
            let id = registered.provider.id();
            if let Some(outcome) = self.blocked(registered, QueryKind::Release, &keys) {
                report(&mut state, Step::UserUrl, Some(id), outcome);
                continue;
            }
            let lookup = self.lookup(registered, &keys);
            match registered.provider.lookup(&Query::Release(url), &lookup) {
                Ok(found) if found.is_empty() => {}
                Ok(found) => {
                    let count = found.len();
                    state
                        .collected
                        .extend(found.into_iter().map(|candidate| Candidate {
                            user_supplied: true,
                            ..candidate
                        }));
                    let best = self.settle(&mut state);
                    report(&mut state, Step::UserUrl, Some(id), ran(count, best));
                }
                Err(error) => report(
                    &mut state,
                    Step::UserUrl,
                    Some(id),
                    StepOutcome::Failed(error.to_string()),
                ),
            }
        }
        let linked = self.linked_releases(url, &keys);
        if !linked.is_empty() {
            state
                .collected
                .extend(linked.into_iter().map(|c| Candidate {
                    user_supplied: true,
                    ..c
                }));
            self.settle(&mut state);
        }
        if !state
            .identification
            .steps
            .iter()
            .any(|report| report.step == Step::UserUrl)
        {
            report(&mut state, Step::UserUrl, None, ran(0, None));
        }
        state.identification
    }
}

fn raw_page_url(url: &ReleaseUrl) -> Option<String> {
    match url {
        ReleaseUrl::Bandcamp(s) | ReleaseUrl::SoundCloud(s) => Some(s.clone()),
        ReleaseUrl::YouTube(id) => Some(format!("https://www.youtube.com/watch?v={id}")),
        _ => None,
    }
}

pub(crate) fn ran(candidates: usize, best: Option<f64>) -> StepOutcome {
    StepOutcome::Ran { candidates, best }
}

pub(crate) fn report(
    state: &mut TrackState,
    step: Step,
    provider: Option<&'static str>,
    outcome: StepOutcome,
) {
    state.identification.steps.push(StepReport {
        step,
        provider,
        outcome,
    });
}

fn upsert_step_report(
    state: &mut TrackState,
    step: Step,
    provider: &'static str,
    outcome: StepOutcome,
) {
    if let Some(existing) = state
        .identification
        .steps
        .iter_mut()
        .find(|r| r.step == step && r.provider == Some(provider))
    {
        existing.outcome = outcome;
    } else {
        report(state, step, Some(provider), outcome);
    }
}
