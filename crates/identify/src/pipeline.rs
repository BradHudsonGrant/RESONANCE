//! The six steps, run in order for a batch of tracks, stopping for each track as soon as one
//! candidate is confident.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use resonance_core::identify::{
    Candidate, FINGERPRINT_MATCH_SCORE, KnownTags, RankedCandidate, ReleaseUrl, STOP_THRESHOLD,
    Step, StepOutcome, confirm_by_fingerprint, normalise_tags, parse_path, rank,
};
use resonance_core::providers::ProviderKey;
use resonance_db::{Database, IdentificationInput};

use crate::credentials::{CredentialError, CredentialStore, ProviderSecret};
use crate::http::HttpClient;
use crate::provider::{
    FetchCounts, Fetcher, IdentifyError, Lookup, MetadataProvider, Query, QueryKind,
};
use crate::providers::{AcoustId, Bandcamp, Discogs, MusicBrainz};
use crate::rate::RateLimiter;

/// How many fingerprint matches are followed to their releases on MusicBrainz.
const RECORDINGS_FOLLOWED: usize = 2;

/// What a step did for a track, and which provider did it.
#[derive(Debug, Clone, PartialEq)]
pub struct StepReport {
    pub step: Step,
    pub provider: Option<&'static str>,
    pub outcome: StepOutcome,
}

/// The answer for one track.
#[derive(Debug, Clone, PartialEq)]
pub struct Identification {
    pub track_id: i64,
    pub known: KnownTags,
    pub best: Option<RankedCandidate>,
    pub steps: Vec<StepReport>,
}

impl Identification {
    pub fn is_confident(&self) -> bool {
        self.best
            .as_ref()
            .is_some_and(|best| best.confidence >= STOP_THRESHOLD)
    }

    /// Every step that applied ran. A step that failed or lacked its key is tried again on the
    /// next run, which costs nothing for the steps whose answers are cached.
    pub fn is_complete(&self) -> bool {
        !self.steps.iter().any(|report| {
            matches!(
                report.outcome,
                StepOutcome::Failed(_) | StepOutcome::SkippedMissingKey(_)
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
    Identification {
        track_id: input.track_id,
        known,
        best: None,
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

struct Registered {
    provider: Box<dyn MetadataProvider>,
    limiter: RateLimiter,
}

/// The providers, each with its own limiter, and the cache they share.
pub struct Pipeline {
    registered: Vec<Registered>,
    fetcher: Fetcher,
    credentials: Arc<dyn CredentialStore>,
    stop_threshold: f64,
}

struct TrackState {
    identification: Identification,
    duration_ms: Option<u64>,
    fingerprint: Option<(String, u32)>,
    fingerprinted: Vec<Candidate>,
    searched: Vec<Candidate>,
    done: bool,
    confident_from_fingerprint: bool,
}

type Keys = HashMap<ProviderKey, Result<Option<ProviderSecret>, CredentialError>>;

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
                    provider,
                })
                .collect(),
            fetcher: Fetcher::new(http, database),
            credentials,
            stop_threshold: STOP_THRESHOLD,
        }
    }

    /// AcoustID, MusicBrainz, Discogs and Bandcamp.
    pub fn with_free_providers(
        http: Arc<dyn HttpClient>,
        database: Arc<Mutex<Database>>,
        credentials: Arc<dyn CredentialStore>,
    ) -> Self {
        Self::new(
            vec![
                Box::new(AcoustId),
                Box::new(MusicBrainz),
                Box::new(Discogs),
                Box::new(Bandcamp),
            ],
            http,
            database,
            credentials,
        )
    }

    /// The picture at `url`, fetched at the pace of the provider that named it.
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
            .fetch_picture(registered.provider.id(), &registered.limiter, url)
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

    fn lookup<'a>(&'a self, registered: &'a Registered, keys: &'a Keys) -> Lookup<'a> {
        Lookup {
            provider: registered.provider.id(),
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

    // Why a provider cannot take a kind of query, if it cannot.
    fn blocked(
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

    fn answering(&self, kind: QueryKind) -> impl Iterator<Item = &Registered> {
        self.registered
            .iter()
            .filter(move |registered| registered.provider.answers(kind))
    }

    /// Runs steps 1 to 6 for each input. Stops early when `abort` is set; the caller should
    /// then drop the results, which are incomplete.
    pub fn identify(
        &self,
        inputs: &[IdentificationInput],
        abort: &AtomicBool,
    ) -> Vec<Identification> {
        let keys = self.keys();
        let mut states: Vec<TrackState> = inputs
            .iter()
            .map(|input| TrackState {
                identification: identify_locally(input),
                duration_ms: input.duration_ms,
                fingerprint: input
                    .fingerprint
                    .clone()
                    .zip(input.fingerprint_duration_s.filter(|seconds| *seconds > 0)),
                fingerprinted: Vec::new(),
                searched: Vec::new(),
                done: false,
                confident_from_fingerprint: false,
            })
            .collect();

        self.fingerprint_step(&mut states, &keys, abort);
        self.text_step(&mut states, &keys, abort);
        for state in &mut states {
            self.confirmation_step(state);
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

    fn fingerprint_step(&self, states: &mut [TrackState], keys: &Keys, abort: &AtomicBool) {
        for registered in self.answering(QueryKind::Fingerprint) {
            let id = registered.provider.id();
            if let Some(outcome) = self.blocked(registered, QueryKind::Fingerprint, keys) {
                for state in states.iter_mut() {
                    report(state, Step::Fingerprint, Some(id), outcome.clone());
                }
                continue;
            }
            for state in states
                .iter_mut()
                .filter(|state| state.fingerprint.is_none())
            {
                report(
                    state,
                    Step::Fingerprint,
                    Some(id),
                    StepOutcome::SkippedNoInput,
                );
            }
            let asked: Vec<usize> = (0..states.len())
                .filter(|index| states[*index].fingerprint.is_some())
                .collect();
            if asked.is_empty() || abort.load(Ordering::Acquire) {
                continue;
            }
            let answers = {
                let queries: Vec<Query<'_>> = asked
                    .iter()
                    .filter_map(|index| states[*index].fingerprint.as_ref())
                    .map(|(fingerprint, duration_s)| Query::Fingerprint {
                        fingerprint,
                        duration_s: *duration_s,
                    })
                    .collect();
                let lookup = self.lookup(registered, keys);
                registered.provider.lookup_batch(&queries, &lookup)
            };
            for (index, answer) in asked.into_iter().zip(answers) {
                let state = &mut states[index];
                match answer {
                    Ok(found) => {
                        let count = found.len();
                        state.fingerprinted.extend(found);
                        report(state, Step::Fingerprint, Some(id), ran(count, None));
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

        for state in states.iter_mut() {
            if abort.load(Ordering::Acquire) {
                return;
            }
            if state.fingerprinted.is_empty() {
                continue;
            }
            self.follow_recordings(state, keys);
            let best = self.settle(state, state.fingerprinted.clone());
            state.confident_from_fingerprint = state.done;
            if let Some(report) = state.identification.steps.iter_mut().rev().find(|report| {
                report.step == Step::Fingerprint
                    && matches!(report.outcome, StepOutcome::Ran { .. })
            }) && let StepOutcome::Ran { best: slot, .. } = &mut report.outcome
            {
                *slot = best;
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
                match registered.provider.lookup(&Query::Text(&known), &lookup) {
                    Ok(found) => {
                        let count = found.len();
                        let best = self.settle(state, found.clone());
                        state.searched.extend(found);
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

    // Needs no network, so it runs even when text search was already confident: that is how a
    // result found by name learns that the audio matches too.
    fn confirmation_step(&self, state: &mut TrackState) {
        if state.confident_from_fingerprint {
            report(
                state,
                Step::AudioConfirmation,
                None,
                StepOutcome::SkippedConfident,
            );
            return;
        }
        if state.fingerprinted.is_empty() {
            report(
                state,
                Step::AudioConfirmation,
                None,
                StepOutcome::SkippedNoInput,
            );
            return;
        }
        let mut searched = std::mem::take(&mut state.searched);
        confirm_by_fingerprint(&mut searched, &state.fingerprinted);
        let confirmed = searched
            .iter()
            .filter(|candidate| candidate.fingerprint_score.is_some())
            .count();
        let mut everything = state.fingerprinted.clone();
        everything.extend(searched);
        let best = self.settle(state, everything);
        report(state, Step::AudioConfirmation, None, ran(confirmed, best));
    }

    // Ranks `candidates`, keeps the best if it beats what the track has, and marks the track
    // done once it is confident.
    fn settle(&self, state: &mut TrackState, candidates: Vec<Candidate>) -> Option<f64> {
        let best = rank(&state.identification.known, state.duration_ms, candidates)
            .into_iter()
            .next()?;
        let confidence = best.confidence;
        let better = state.identification.best.as_ref().is_none_or(|current| {
            confidence > current.confidence
                || (confidence == current.confidence
                    && best.audio_matched
                    && !current.audio_matched)
        });
        if better {
            state.identification.best = Some(best);
        }
        if confidence >= self.stop_threshold {
            state.done = true;
        }
        Some(confidence)
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
            searched: Vec::new(),
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
                    let pasted = found
                        .into_iter()
                        .map(|candidate| Candidate {
                            user_supplied: true,
                            ..candidate
                        })
                        .collect();
                    let best = self.settle(&mut state, pasted);
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

fn ran(candidates: usize, best: Option<f64>) -> StepOutcome {
    StepOutcome::Ran { candidates, best }
}

fn report(
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
