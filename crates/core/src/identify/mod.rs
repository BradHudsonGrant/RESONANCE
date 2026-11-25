//! The rules for working out what a file is: reading what the file and its path already say,
//! and ranking what providers propose against that. Nothing here touches the network or a disk.

mod fuse;
mod parse;
mod score;
mod url;

pub use fuse::{
    EDIT_LENGTH_GAP_MS, ENRICHMENT_THRESHOLD, FINGERPRINT_PREFIX_MS, LONGER_EDIT_MIX,
    MIN_ALTERNATIVE_CONFIDENCE, MIN_HIGH_CONF_ALTERNATIVE, MIN_TAGS_ONLY_CONFIDENCE,
    MIN_VIABLE_CONFIDENCE, ProviderLineage, build_match_evidence, fuse_and_rank, is_other_edit,
    is_same_candidate, is_viable_alternative, is_viable_candidate, needs_preview_check,
    replaces_best, retain_viable_candidates, should_expand_candidates, visible_candidates,
};
pub use parse::{
    artist_search_terms, clean_release_title, clean_text, extract_isrc, find_catno, fold_latin,
    is_generic_distributor_label, is_placeholder_artist, normalise_catno_tag, normalise_label,
    normalise_tags, normalise_tokens, parse_path, release_seller, search_fallback_text,
    search_title_clean, similarity, split_artist_from_title, split_featured, split_mix,
    strip_promo_and_catno, take_release_hint,
};
pub use score::{
    MixFamily, RemixQualifier, classify_mix, confirm_by_fingerprint, mix_family_matches,
    mix_search_hint, rank, score, score_confidence,
};
pub use url::{
    MAX_GUESSED_BANDCAMP_PAGES, MatchedPage, ReleaseUrl, UrlRejected, bandcamp_seller,
    extract_release_urls, guessed_bandcamp_pages, learned_sellers, mix_maker, parse_release_url,
};

use crate::library::TagField;
use crate::providers::ProviderKey;

/// The cascade stops as soon as the best candidate reaches this.
pub const STOP_THRESHOLD: f64 = 0.92;

/// What a release page the user pasted is worth, unless the audio plainly disagrees with it.
pub const USER_URL_CONFIDENCE: f64 = 0.95;

/// Baseline worth of a release page found inside a file comment tag until confirmed by tags or length.
pub const EMBEDDED_URL_CONFIDENCE: f64 = 0.88;

/// An AcoustID score at or above this means Chromaprint compared the audio and it matched.
pub const FINGERPRINT_MATCH_SCORE: f64 = 0.7;

/// Minimum sliding-window Chromaprint similarity for a 30 s preview confirmation.
pub const PREVIEW_MATCH_SCORE: f32 = 0.82;

/// How many ranked alternatives identification keeps for Prep candidate switching.
pub const MAX_STORED_CANDIDATES: usize = 5;

/// Providers whose cover images are cleared for storage and file embedding.
pub const EMBEDDABLE_ARTWORK_PROVIDERS: [&str; 4] =
    ["musicbrainz", "discogs", "bandcamp", "soundcloud"];

/// The six steps, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Step {
    ExistingTags,
    FileName,
    Fingerprint,
    TextSearch,
    AudioConfirmation,
    UserUrl,
}

impl Step {
    pub const ALL: [Self; 6] = [
        Self::ExistingTags,
        Self::FileName,
        Self::Fingerprint,
        Self::TextSearch,
        Self::AudioConfirmation,
        Self::UserUrl,
    ];

    /// Steps 3 onwards ask a provider; the first two only read the file.
    pub fn is_online(self) -> bool {
        self.number() >= 3
    }

    pub fn number(self) -> u8 {
        match self {
            Self::ExistingTags => 1,
            Self::FileName => 2,
            Self::Fingerprint => 3,
            Self::TextSearch => 4,
            Self::AudioConfirmation => 5,
            Self::UserUrl => 6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::ExistingTags => "existing_tags",
            Self::FileName => "file_name",
            Self::Fingerprint => "fingerprint",
            Self::TextSearch => "text_search",
            Self::AudioConfirmation => "audio_confirmation",
            Self::UserUrl => "user_url",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|step| step.name() == name)
    }
}

/// True when at least one online identification step (step 3 or later) has been attempted.
pub fn has_online_attempt(steps: &[Step]) -> bool {
    steps.iter().any(|step| step.is_online())
}

/// What a step did for one track, as the user sees it in the report.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// Steps 1 and 2: how many fields the tags or the path gave.
    Read { fields: usize },
    /// The step ran. `best` is the best confidence among what it found.
    Ran {
        candidates: usize,
        best: Option<f64>,
    },
    /// The step needs a key the user has not entered in Settings.
    SkippedMissingKey(ProviderKey),
    /// The step had nothing to work with, such as no fingerprint or no artist and title.
    SkippedNoInput,
    /// An earlier step was already confident.
    SkippedConfident,
    /// The provider could not be reached or answered with an error. The run goes on.
    Failed(String),
    /// Nothing was confident, so the user is asked for a release URL.
    NeedsUrl,
}

/// What release format a candidate came from, used to prefer originals over later compilations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReleaseKind {
    SingleOrEp,
    Album,
    Compilation,
    #[default]
    Unknown,
}

impl ReleaseKind {
    pub(crate) fn tie_rank(self) -> u8 {
        match self {
            Self::SingleOrEp => 3,
            Self::Album => 2,
            Self::Unknown => 1,
            Self::Compilation => 0,
        }
    }
}

/// Which audio verification method confirmed a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMatchSource {
    Fingerprint,
    PreviewMatch,
    Recogniser,
}

/// Primary reason the winning candidate scored as it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchBasis {
    AudioFingerprint,
    AudioPreview,
    AudioRecogniser,
    UserUrl,
    EmbeddedUrl,
    CatalogAndDuration,
    TagsAndDuration,
    #[default]
    TagsOnly,
    LocalTags,
}

/// Structured evidence presented beside a match in Prep.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MatchEvidence {
    pub basis: MatchBasis,
    pub providers: Vec<String>,
    pub primary_provider: String,
    pub duration_delta_ms: Option<u64>,
    pub preview_similarity: Option<f32>,
    pub catno_matched: bool,
    pub isrc_matched: bool,
    pub url: Option<String>,
}

/// What a file says about itself, through its tags or its path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownTags {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub mix: Option<String>,
    pub album: Option<String>,
    pub label: Option<String>,
    pub catno: Option<String>,
    pub year: Option<u16>,
    pub track_number: Option<String>,
    pub remixer: Option<String>,
    pub isrc: Option<String>,
    pub embedded_urls: Vec<ReleaseUrl>,
    pub artist_inferred_from_title: bool,
    /// Bandcamp sellers other tracks by this artist were matched to, tried before guessing.
    pub bandcamp_sellers: Vec<String>,
}

impl KnownTags {
    /// Takes each field `self` lacks from `other`. What the tags say wins over the path.
    pub fn fill_from(&mut self, other: &KnownTags) {
        fn fill(target: &mut Option<String>, source: &Option<String>) {
            if target.is_none() {
                target.clone_from(source);
            }
        }
        if self.artist.is_none() && other.artist.is_some() {
            self.artist.clone_from(&other.artist);
            self.artist_inferred_from_title = other.artist_inferred_from_title;
        }
        fill(&mut self.title, &other.title);
        fill(&mut self.mix, &other.mix);
        fill(&mut self.album, &other.album);
        fill(&mut self.label, &other.label);
        fill(&mut self.catno, &other.catno);
        fill(&mut self.track_number, &other.track_number);
        fill(&mut self.remixer, &other.remixer);
        fill(&mut self.isrc, &other.isrc);
        if self.year.is_none() {
            self.year = other.year;
        }
        for url in &other.embedded_urls {
            if !self.embedded_urls.contains(url) {
                self.embedded_urls.push(url.clone());
            }
        }
    }

    /// Enough to search a provider's catalogue by text, catalogue number, ISRC or multi-word title.
    pub fn is_searchable(&self) -> bool {
        (self.artist.is_some() && self.title.is_some())
            || self.catno.is_some()
            || self.isrc.is_some()
            || (self.artist.is_none()
                && self
                    .title
                    .as_deref()
                    .is_some_and(|title| normalise_tokens(title).len() >= 3))
    }

    /// How many fields are known.
    pub fn field_count(&self) -> usize {
        [
            &self.artist,
            &self.title,
            &self.mix,
            &self.album,
            &self.label,
            &self.catno,
            &self.track_number,
            &self.remixer,
            &self.isrc,
        ]
        .into_iter()
        .filter(|field| field.is_some())
        .count()
            + usize::from(self.year.is_some())
    }
}

/// One answer a provider gives for what a file might be.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Candidate {
    pub provider: String,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub mix: Option<String>,
    pub album: Option<String>,
    pub label: Option<String>,
    pub catno: Option<String>,
    pub year: Option<u16>,
    pub duration_ms: Option<u64>,
    pub genre: Option<String>,
    pub subgenre: Option<String>,
    pub track_number: Option<String>,
    pub isrc: Option<String>,
    /// MusicBrainz recording MBID, when the candidate is a MusicBrainz recording.
    pub recording_id: Option<String>,
    pub release_id: Option<String>,
    pub url: Option<String>,
    /// A picture of the release's front cover, only from embeddable sources.
    pub artwork_url: Option<String>,
    /// Which provider owns `artwork_url`, so fused candidates fetch from the right source.
    pub artwork_provider: Option<String>,
    /// 30-second MP3 or M4A stream URL verified in memory during step 5. Never persisted to disk.
    pub preview_url: Option<String>,
    pub release_kind: ReleaseKind,
    /// Set only when Chromaprint compared this file's full audio with AcoustID's index.
    pub fingerprint_score: Option<f64>,
    /// Set only when sliding-window Chromaprint matched a 30-second preview in memory.
    pub preview_similarity: Option<f32>,
    /// Set only when an audio check directly matched the audio.
    pub audio_source: Option<AudioMatchSource>,
    /// From a release page the user pasted.
    pub user_supplied: bool,
    /// From a release link stored in the file's own comments.
    pub from_embedded_url: bool,
    /// The artist is the name of the account selling the release, which may be a label, so it is
    /// never suggested in place of the file's artist.
    pub artist_is_seller: bool,
    /// Per-field winning provider recorded by fusion.
    pub field_providers: Vec<(TagField, String)>,
}

impl Candidate {
    pub fn provider_for(&self, field: TagField) -> &str {
        self.field_providers
            .iter()
            .find(|(held, _)| *held == field)
            .map_or(self.provider.as_str(), |(_, name)| name.as_str())
    }

    pub fn artwork_fetcher_provider(&self) -> Option<&str> {
        let _ = self.artwork_url.as_deref()?;
        let source = self
            .artwork_provider
            .as_deref()
            .unwrap_or(self.provider.as_str());
        EMBEDDABLE_ARTWORK_PROVIDERS
            .contains(&source)
            .then_some(source)
    }
}

/// A candidate with its confidence for one file.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub candidate: Candidate,
    pub confidence: f64,
    /// True only when Chromaprint or an audio recogniser directly matched the audio.
    pub audio_matched: bool,
    pub duration_delta_ms: Option<u64>,
    pub providers: Vec<String>,
    pub evidence: MatchEvidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_round_trip_by_name_and_number() {
        for (index, step) in Step::ALL.into_iter().enumerate() {
            assert_eq!(Step::from_name(step.name()), Some(step));
            assert_eq!(usize::from(step.number()), index + 1);
        }
    }

    #[test]
    fn embeddable_artwork_allowlist_excludes_deezer_and_itunes() {
        let deezer = Candidate {
            provider: "deezer".to_owned(),
            artwork_url: Some("https://cdn-images.dzcdn.net/cover.jpg".to_owned()),
            ..Candidate::default()
        };
        assert_eq!(deezer.artwork_fetcher_provider(), None);

        let fused_with_caa = Candidate {
            provider: "discogs".to_owned(),
            artwork_url: Some("https://coverartarchive.org/release/x/front-500".to_owned()),
            artwork_provider: Some("musicbrainz".to_owned()),
            ..Candidate::default()
        };
        assert_eq!(
            fused_with_caa.artwork_fetcher_provider(),
            Some("musicbrainz")
        );
    }

    #[test]
    fn has_online_attempt_ignores_local_steps_and_detects_step_three_onward() {
        assert!(!has_online_attempt(&[Step::ExistingTags, Step::FileName]));
        assert!(has_online_attempt(&[
            Step::ExistingTags,
            Step::FileName,
            Step::TextSearch
        ]));
    }
}
