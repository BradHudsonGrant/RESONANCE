//! Scoring individual provider candidates against a track's tags, duration and release facts.

use super::parse::{
    fold_latin, is_generic_distributor_label, is_placeholder_artist, normalise_label,
    normalise_tokens, search_title_clean, similarity, split_featured, split_mix,
};
use super::{
    AudioMatchSource, Candidate, EMBEDDED_URL_CONFIDENCE, FINGERPRINT_MATCH_SCORE, KnownTags,
    MatchBasis, MatchEvidence, PREVIEW_MATCH_SCORE, RankedCandidate, USER_URL_CONFIDENCE,
};

/// Within this, a candidate's duration counts as the same edit.
const SAME_EDIT_SECONDS: f64 = 2.0;

/// Past this without audio/catalogue confirmation, a candidate is a different edit.
const DIFFERENT_EDIT_SECONDS: f64 = 12.0;

/// Past this with non-Same mix, a candidate is a different edit.
const DIFFERENT_EDIT_MISMATCHED_MIX_SECONDS: f64 = 10.0;

/// Maximum vinyl pressing / rip drift tolerated when audio or exact catalogue confirms the track.
const VINYL_DRIFT_MAX_SECONDS: f64 = 25.0;

/// The most a candidate of a different edit or mix can score, so it never wins on its own.
pub(crate) const DIFFERENT_VERSION_CAP: f64 = 0.30;
pub(crate) const ZERO_TEXT_CAP: f64 = 0.25;
pub(crate) const ONE_SIDED_SIGNIFICANT_CAP: f64 = 0.85;
pub(crate) const MISSING_ARTIST_CAP: f64 = 0.80;
pub(crate) const MISSING_DURATION_CAP: f64 = 0.88;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemixQualifier {
    Vocal,
    Dub,
    Instrumental,
    Radio,
    Acapella,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixFamily {
    Original,
    Extended,
    Radio,
    Dub,
    Instrumental,
    Acapella,
    Vip,
    NamedRemix {
        tokens: Vec<String>,
        qualifiers: Vec<RemixQualifier>,
        has_joiner: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MixAgreement {
    Same,
    OneSided,
    OneSidedSignificant,
    Different,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatnoMatch {
    Exact,
    SameFamily,
    Different,
}

/// Hard safety caps computed during individual scoring and re-applied after cross-provider fusion.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ScoreGuards {
    pub different_version: bool,
    pub zero_text: bool,
    pub one_sided_significant: bool,
    pub missing_artist: bool,
    pub missing_duration: bool,
    pub catno_matched: bool,
    pub isrc_matched: bool,
    pub text_similarity: f64,
    pub audio_matched: bool,
}

impl ScoreGuards {
    pub(crate) fn apply_caps(self, mut confidence: f64, user_supplied: bool) -> f64 {
        if self.zero_text && !self.audio_matched && !self.isrc_matched && !user_supplied {
            confidence = confidence.min(ZERO_TEXT_CAP);
        }
        if self.one_sided_significant && !self.audio_matched && !self.isrc_matched && !user_supplied
        {
            confidence = confidence.min(ONE_SIDED_SIGNIFICANT_CAP);
        }
        if self.missing_artist
            && !self.catno_matched
            && !self.isrc_matched
            && !self.audio_matched
            && !user_supplied
        {
            confidence = confidence.min(MISSING_ARTIST_CAP);
        }
        if self.missing_duration
            && !self.catno_matched
            && !self.isrc_matched
            && !self.audio_matched
            && !user_supplied
        {
            confidence = confidence.min(MISSING_DURATION_CAP);
        }
        if self.different_version {
            confidence = confidence.min(DIFFERENT_VERSION_CAP);
        }
        confidence.clamp(0.0, 1.0)
    }

    pub(crate) fn merge_from(&mut self, other: Self) {
        self.different_version |= other.different_version;
        self.zero_text &= other.zero_text;
        self.one_sided_significant &= other.one_sided_significant;
        self.missing_artist &= other.missing_artist;
        self.missing_duration &= other.missing_duration;
        self.catno_matched |= other.catno_matched;
        self.isrc_matched |= other.isrc_matched;
        self.audio_matched |= other.audio_matched;
        self.text_similarity = self.text_similarity.max(other.text_similarity);
    }
}

/// Step 5a: a text-search candidate whose MusicBrainz recording came back from AcoustID is
/// confirmed by the audio.
pub fn confirm_by_fingerprint(candidates: &mut [Candidate], fingerprinted: &[Candidate]) {
    for candidate in candidates.iter_mut() {
        if candidate.fingerprint_score.is_some() {
            continue;
        }
        let Some(recording) = &candidate.recording_id else {
            continue;
        };
        candidate.fingerprint_score = fingerprinted
            .iter()
            .filter(|matched| matched.recording_id.as_ref() == Some(recording))
            .filter_map(|matched| matched.fingerprint_score)
            .reduce(f64::max);
        if candidate
            .fingerprint_score
            .is_some_and(|score| score >= FINGERPRINT_MATCH_SCORE)
            && candidate.audio_source.is_none()
        {
            candidate.audio_source = Some(AudioMatchSource::Fingerprint);
        }
    }
}

/// Scores every candidate against what the file says and how long it is, best first.
pub fn rank(
    known: &KnownTags,
    duration_ms: Option<u64>,
    candidates: Vec<Candidate>,
) -> Vec<RankedCandidate> {
    let mut ranked: Vec<_> = candidates
        .into_iter()
        .map(|candidate| score(known, duration_ms, candidate))
        .collect();
    ranked.sort_by(compare_ranked);
    ranked
}

pub(crate) fn compare_ranked(a: &RankedCandidate, b: &RankedCandidate) -> std::cmp::Ordering {
    b.confidence
        .total_cmp(&a.confidence)
        .then_with(|| b.audio_matched.cmp(&a.audio_matched))
        .then_with(|| {
            b.duration_delta_ms
                .is_some()
                .cmp(&a.duration_delta_ms.is_some())
        })
        .then_with(|| {
            b.candidate
                .release_kind
                .tie_rank()
                .cmp(&a.candidate.release_kind.tie_rank())
        })
        .then_with(|| {
            a.duration_delta_ms
                .unwrap_or(u64::MAX)
                .cmp(&b.duration_delta_ms.unwrap_or(u64::MAX))
        })
}

/// Weighs duration, Chromaprint / preview confirmation, title and artist, mix name, ISRC,
/// label and catalogue number.
pub fn score(known: &KnownTags, duration_ms: Option<u64>, candidate: Candidate) -> RankedCandidate {
    score_with_guards(known, duration_ms, candidate).0
}

/// Computes only the final confidence score without moving or cloning `candidate`.
pub fn score_confidence(known: &KnownTags, duration_ms: Option<u64>, candidate: &Candidate) -> f64 {
    score_parts(known, duration_ms, candidate).confidence
}

struct ScoreParts {
    confidence: f64,
    audio_matched: bool,
    duration_delta_ms: Option<u64>,
    catno_matched: bool,
    isrc_matched: bool,
    guards: ScoreGuards,
}

pub(crate) fn score_with_guards(
    known: &KnownTags,
    duration_ms: Option<u64>,
    candidate: Candidate,
) -> (RankedCandidate, ScoreGuards) {
    let parts = score_parts(known, duration_ms, &candidate);
    let primary_provider = candidate.provider.clone();
    let basis = classify_match_basis(
        parts.audio_matched,
        candidate.audio_source,
        candidate.user_supplied,
        candidate.from_embedded_url,
        parts.catno_matched,
        parts.duration_delta_ms,
    );

    let match_evidence = MatchEvidence {
        basis,
        providers: vec![primary_provider.clone()],
        primary_provider,
        duration_delta_ms: parts.duration_delta_ms,
        preview_similarity: candidate.preview_similarity,
        catno_matched: parts.catno_matched,
        isrc_matched: parts.isrc_matched,
        url: candidate.url.clone(),
    };

    let ranked = RankedCandidate {
        providers: match_evidence.providers.clone(),
        candidate,
        confidence: parts.confidence,
        audio_matched: parts.audio_matched,
        duration_delta_ms: parts.duration_delta_ms,
        evidence: match_evidence,
    };
    (ranked, parts.guards)
}

fn score_parts(known: &KnownTags, duration_ms: Option<u64>, candidate: &Candidate) -> ScoreParts {
    let duration_delta_ms = duration_ms
        .zip(candidate.duration_ms)
        .map(|(ours, theirs)| ours.abs_diff(theirs));
    let delta_s = duration_delta_ms.map(|delta| delta as f64 / 1000.0);

    let (candidate_title, candidate_mix) = match (&candidate.title, &candidate.mix) {
        (Some(title), None) => {
            let (clean, mix) = split_mix(title);
            (Some(clean), mix)
        }
        (Some(title), Some(mix)) => {
            let (clean, _) = split_mix(title);
            (Some(clean), Some(mix.clone()))
        }
        (None, mix) => (None, mix.clone()),
    };

    let mix = mix_agreement_with_known(known, candidate, candidate_mix.as_deref());
    let isrc_matched = matches_isrc(known.isrc.as_deref(), candidate.isrc.as_deref());
    let catno_kind = catno_match(known.catno.as_deref(), candidate.catno.as_deref());
    let catno_exact = catno_kind == CatnoMatch::Exact;
    let label_same = labels_match(known.label.as_deref(), candidate.label.as_deref());
    let catno_matched = catno_exact || (catno_kind == CatnoMatch::SameFamily && label_same);

    let raw_audio_match = candidate
        .fingerprint_score
        .is_some_and(|score| score >= FINGERPRINT_MATCH_SCORE)
        || candidate
            .preview_similarity
            .is_some_and(|score| score >= PREVIEW_MATCH_SCORE)
        || candidate.audio_source == Some(AudioMatchSource::Recogniser);

    let edit_limit = if mix == MixAgreement::Same {
        if raw_audio_match || catno_exact || isrc_matched {
            VINYL_DRIFT_MAX_SECONDS
        } else {
            DIFFERENT_EDIT_SECONDS
        }
    } else {
        DIFFERENT_EDIT_MISMATCHED_MIX_SECONDS
    };
    let different_edit = delta_s.is_some_and(|s| s > edit_limit);
    let different_version = different_edit || mix == MixAgreement::Different;

    let duration = delta_s.map(|s| duration_score(s, edit_limit));
    let title_sim = title_similarity(known.title.as_deref(), candidate_title.as_deref());
    let artist_sim = artist_similarity(known.artist.as_deref(), candidate.artist.as_deref());
    let remixer_is_candidate_artist = match classify_mix(known.mix.as_deref()) {
        MixFamily::NamedRemix { ref tokens, .. } => {
            field_credits_remixer(candidate.artist.as_deref(), tokens, false)
        }
        _ => false,
    };
    let effective_artist_sim = if remixer_is_candidate_artist {
        artist_sim.map(|sim| sim.max(0.85))
    } else {
        artist_sim
    };
    let text = text_score_from_sims(title_sim, effective_artist_sim);
    let release = release_score(known, candidate, catno_kind, label_same, isrc_matched);

    let parts = [(text, 0.50), (duration, 0.25), (release, 0.25)];
    let weight: f64 = parts
        .iter()
        .filter(|(v, _)| v.is_some())
        .map(|(_, w)| w)
        .sum();
    let mut evidence = if weight > 0.0 {
        parts
            .iter()
            .filter_map(|(value, w)| value.map(|value| value * w))
            .sum::<f64>()
            / weight
    } else {
        0.0
    };

    if isrc_matched && !different_version {
        evidence = evidence.max(0.94);
    }

    match mix {
        MixAgreement::OneSided => {
            evidence *= if duration.is_some() { 0.97 } else { 0.85 };
        }
        MixAgreement::OneSidedSignificant => {
            evidence *= 0.88;
        }
        MixAgreement::Same | MixAgreement::Different => {}
    }

    // Preview/recogniser audio confirmation never marks a mismatched mix or edit as audio_matched.
    let fp_confirmed = candidate
        .fingerprint_score
        .is_some_and(|score| score >= FINGERPRINT_MATCH_SCORE);
    let audio_matched = fp_confirmed || (raw_audio_match && !different_version);

    let audio_boost = if fp_confirmed {
        candidate.fingerprint_score
    } else if audio_matched {
        match candidate.audio_source {
            Some(AudioMatchSource::PreviewMatch) => candidate
                .preview_similarity
                .map(|s| f64::from(s).clamp(0.85, 0.96)),
            Some(AudioMatchSource::Recogniser) => Some(0.92),
            _ => candidate.fingerprint_score,
        }
    } else {
        None
    };

    let mut confidence = match audio_boost {
        Some(boost) if audio_matched => evidence + (1.0 - evidence) * boost,
        _ => evidence,
    };

    if candidate.user_supplied && !different_version {
        confidence = confidence.max(USER_URL_CONFIDENCE);
    } else if candidate.from_embedded_url && !different_version {
        confidence = confidence.max(EMBEDDED_URL_CONFIDENCE);
    }

    let text_similarity = text.unwrap_or(0.0);
    let zero_title = known.title.is_some()
        && candidate_title.is_some()
        && title_sim.is_some_and(|sim| sim < 0.10);
    let both_named_artists = known
        .artist
        .as_deref()
        .is_some_and(|a| !is_placeholder_artist(a) && !normalise_tokens(a).is_empty())
        && candidate
            .artist
            .as_deref()
            .is_some_and(|b| !is_placeholder_artist(b) && !normalise_tokens(b).is_empty());
    let zero_artist = both_named_artists
        && !remixer_is_candidate_artist
        && artist_sim.is_some_and(|sim| sim < 0.10);
    let zero_text = zero_title
        || zero_artist
        || (known.title.is_some()
            && known.artist.is_some()
            && candidate.title.is_some()
            && candidate.artist.is_some()
            && text_similarity < 0.05);

    let guards = ScoreGuards {
        different_version,
        zero_text,
        one_sided_significant: mix == MixAgreement::OneSidedSignificant,
        missing_artist: known.artist.is_none(),
        missing_duration: duration_delta_ms.is_none(),
        catno_matched,
        isrc_matched,
        text_similarity,
        audio_matched,
    };

    let confidence = guards.apply_caps(confidence, candidate.user_supplied);
    ScoreParts {
        confidence,
        audio_matched,
        duration_delta_ms,
        catno_matched,
        isrc_matched,
        guards,
    }
}

pub(crate) fn classify_match_basis(
    audio_matched: bool,
    audio_source: Option<AudioMatchSource>,
    user_supplied: bool,
    from_embedded_url: bool,
    catno_matched: bool,
    duration_delta_ms: Option<u64>,
) -> MatchBasis {
    if audio_matched {
        match audio_source {
            Some(AudioMatchSource::PreviewMatch) => MatchBasis::AudioPreview,
            Some(AudioMatchSource::Recogniser) => MatchBasis::AudioRecogniser,
            _ => MatchBasis::AudioFingerprint,
        }
    } else if user_supplied {
        MatchBasis::UserUrl
    } else if from_embedded_url {
        MatchBasis::EmbeddedUrl
    } else if catno_matched && duration_delta_ms.is_some() {
        MatchBasis::CatalogAndDuration
    } else if duration_delta_ms.is_some() {
        MatchBasis::TagsAndDuration
    } else {
        MatchBasis::TagsOnly
    }
}

fn duration_score(delta_s: f64, limit_s: f64) -> f64 {
    if delta_s <= SAME_EDIT_SECONDS {
        1.0
    } else if delta_s <= limit_s {
        1.0 - 0.6 * (delta_s - SAME_EDIT_SECONDS) / (limit_s - SAME_EDIT_SECONDS).max(1.0)
    } else {
        0.0
    }
}

/// Classifies a mix string into a DJ mix family and extracted named remix tokens.
pub fn classify_mix(mix: Option<&str>) -> MixFamily {
    let Some(raw) = mix.map(str::trim).filter(|m| !m.is_empty()) else {
        return MixFamily::Original;
    };
    let lower = fold_latin(raw);
    let has_joiner = [" & ", " and ", " x ", " vs ", " vs. ", " feat ", " ft "]
        .iter()
        .any(|sep| lower.contains(sep));

    let tokens = normalise_tokens(raw);
    if tokens.is_empty() {
        return MixFamily::Original;
    }

    let as_strs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    match as_strs.as_slice() {
        ["original"] | ["original", "mix"] | ["main"] | ["main", "mix"] | ["album", "version"] => {
            return MixFamily::Original;
        }
        ["extended"]
        | ["extended", "mix"]
        | ["club"]
        | ["club", "mix"]
        | ["extended", "club", "mix"]
        | ["original", "extended", "mix"]
        | ["12", "mix"]
        | ["12", "inch"]
        | ["full", "length"] => return MixFamily::Extended,
        ["radio"]
        | ["radio", "edit"]
        | ["radio", "mix"]
        | ["single", "edit"]
        | ["edit"]
        | ["short", "edit"] => return MixFamily::Radio,
        ["dub"] | ["dub", "mix"] | ["dub", "version"] => return MixFamily::Dub,
        ["instrumental"] | ["instrumental", "mix"] => return MixFamily::Instrumental,
        ["acapella"] | ["accapella"] | ["a", "cappella"] | ["acapella", "mix"] => {
            return MixFamily::Acapella;
        }
        ["vip"] | ["vip", "mix"] | ["vip", "edit"] => return MixFamily::Vip,
        _ => {}
    }

    let mut qualifiers = Vec::new();
    let mut name_tokens = Vec::new();
    for token in tokens {
        match token.as_str() {
            "vocal" | "vocals" => push_unique(&mut qualifiers, RemixQualifier::Vocal),
            "dub" => push_unique(&mut qualifiers, RemixQualifier::Dub),
            "instrumental" => push_unique(&mut qualifiers, RemixQualifier::Instrumental),
            "radio" => push_unique(&mut qualifiers, RemixQualifier::Radio),
            "acapella" | "accapella" => push_unique(&mut qualifiers, RemixQualifier::Acapella),
            "mix" | "remix" | "rmx" | "edit" | "version" | "rework" | "refix" | "rub" | "cut"
            | "extended" | "club" | "original" | "bootleg" | "remake" | "flip" => {}
            _ => name_tokens.push(token),
        }
    }

    if name_tokens.is_empty() {
        return match qualifiers.first().copied() {
            Some(RemixQualifier::Dub) => MixFamily::Dub,
            Some(RemixQualifier::Instrumental) => MixFamily::Instrumental,
            Some(RemixQualifier::Radio) => MixFamily::Radio,
            Some(RemixQualifier::Acapella) => MixFamily::Acapella,
            _ => MixFamily::Original,
        };
    }

    MixFamily::NamedRemix {
        tokens: name_tokens,
        qualifiers,
        has_joiner,
    }
}

fn push_unique<T: PartialEq>(vec: &mut Vec<T>, item: T) {
    if !vec.contains(&item) {
        vec.push(item);
    }
}

const GENERIC_REMIX_STYLE_TOKENS: [&str; 12] = [
    "acid", "club", "dark", "deep", "disco", "funk", "house", "love", "night", "soul", "summer",
    "sunset",
];

fn field_credits_remixer(field: Option<&str>, remixer_tokens: &[String], is_album: bool) -> bool {
    let Some(raw) = field else {
        return false;
    };
    let core: Vec<String> = strip_leading_stopwords(remixer_tokens)
        .iter()
        .filter(|tok| tok.as_str() != "and")
        .cloned()
        .collect();
    if core.is_empty() {
        return false;
    }
    let full: Vec<&str> = remixer_tokens
        .iter()
        .map(String::as_str)
        .filter(|&tok| tok != "and")
        .collect();
    let joined = core.join(" ");
    let full_joined = full.join(" ");
    let tokens = normalise_tokens(raw);
    if tokens.is_empty() {
        return false;
    }
    let best_sim = similarity(raw, &joined).max(similarity(raw, &full_joined));
    if core.len() == 1 {
        if is_album && GENERIC_REMIX_STYLE_TOKENS.contains(&core[0].as_str()) {
            return false;
        }
        return best_sim >= 0.75;
    }
    core.iter()
        .all(|needle| tokens.iter().any(|tok| tokens_match_remix(tok, needle)))
        || best_sim >= 0.65
}

fn candidate_credits_remixer(candidate: &Candidate, remixer_tokens: &[String]) -> bool {
    field_credits_remixer(candidate.artist.as_deref(), remixer_tokens, false)
        || field_credits_remixer(candidate.album.as_deref(), remixer_tokens, true)
}

pub(crate) fn mix_agreement_with_known(
    known: &KnownTags,
    candidate: &Candidate,
    candidate_mix: Option<&str>,
) -> MixAgreement {
    let explicit_original = known.mix.as_deref().is_some_and(|m| {
        let t = normalise_tokens(m);
        !t.is_empty() && classify_mix(Some(m)) == MixFamily::Original
    });
    let ours = classify_mix(known.mix.as_deref());
    let theirs = classify_mix(candidate_mix);

    match (&ours, &theirs) {
        (MixFamily::Original, MixFamily::Original) => MixAgreement::Same,
        (MixFamily::Original, MixFamily::Extended) | (MixFamily::Extended, MixFamily::Original) => {
            MixAgreement::OneSided
        }
        (MixFamily::Original, MixFamily::Radio) if !explicit_original => {
            MixAgreement::OneSidedSignificant
        }
        (MixFamily::Original, MixFamily::NamedRemix { tokens, .. }) if !explicit_original => {
            if remixer_matches_our_file(known, tokens) {
                MixAgreement::OneSided
            } else {
                MixAgreement::OneSidedSignificant
            }
        }
        (MixFamily::Original, _) if explicit_original => MixAgreement::Different,
        (MixFamily::Original, _) => MixAgreement::OneSidedSignificant,
        (MixFamily::Radio, MixFamily::Original) => MixAgreement::OneSidedSignificant,
        (MixFamily::NamedRemix { tokens, .. }, MixFamily::Original) => {
            if candidate_credits_remixer(candidate, tokens) {
                MixAgreement::OneSided
            } else {
                MixAgreement::Different
            }
        }
        (
            MixFamily::Dub | MixFamily::Instrumental | MixFamily::Acapella | MixFamily::Vip,
            MixFamily::Original,
        ) => MixAgreement::Different,
        (
            MixFamily::NamedRemix {
                tokens: a,
                qualifiers: qa,
                has_joiner: ja,
            },
            MixFamily::NamedRemix {
                tokens: b,
                qualifiers: qb,
                has_joiner: jb,
            },
        ) => {
            if qualifiers_conflict(qa, qb) {
                return MixAgreement::Different;
            }
            if named_remix_tokens_match(a, *ja, b, *jb) {
                MixAgreement::Same
            } else {
                MixAgreement::Different
            }
        }
        (a, b) if a == b => MixAgreement::Same,
        _ => MixAgreement::Different,
    }
}

fn qualifiers_conflict(a: &[RemixQualifier], b: &[RemixQualifier]) -> bool {
    const EXCLUSIVE: [RemixQualifier; 4] = [
        RemixQualifier::Dub,
        RemixQualifier::Instrumental,
        RemixQualifier::Radio,
        RemixQualifier::Acapella,
    ];
    EXCLUSIVE.iter().any(|q| a.contains(q) != b.contains(q))
}

const REMIX_STOPWORDS: [&str; 6] = ["dj", "the", "mr", "dr", "and", "of"];

fn strip_leading_stopwords(mut tokens: &[String]) -> &[String] {
    while let Some(first) = tokens.first() {
        if REMIX_STOPWORDS.contains(&first.as_str()) {
            tokens = &tokens[1..];
        } else {
            break;
        }
    }
    tokens
}

fn dedup_words(tokens: &[String]) -> Vec<&str> {
    let mut unique = Vec::with_capacity(tokens.len());
    for token in tokens {
        let s = token.as_str();
        if !unique.contains(&s) {
            unique.push(s);
        }
    }
    unique
}

fn tokens_match_remix(a: &str, b: &str) -> bool {
    a == b
        || a.strip_suffix('s')
            .is_some_and(|stem| stem.len() >= 4 && stem == b)
        || b.strip_suffix('s')
            .is_some_and(|stem| stem.len() >= 4 && stem == a)
}

fn named_remix_tokens_match(a: &[String], ja: bool, b: &[String], jb: bool) -> bool {
    let a_core = strip_leading_stopwords(a);
    let b_core = strip_leading_stopwords(b);

    let set_a = dedup_words(a_core);
    let set_b = dedup_words(b_core);
    if set_a.is_empty() || set_b.is_empty() {
        return false;
    }
    let mut used = vec![false; set_b.len()];
    let mut shared = 0usize;
    for wa in &set_a {
        if let Some(pos) = set_b
            .iter()
            .enumerate()
            .position(|(idx, wb)| !used[idx] && tokens_match_remix(wa, wb))
        {
            used[pos] = true;
            shared += 1;
        }
    }
    let dice = 2.0 * shared as f64 / (set_a.len() + set_b.len()) as f64;
    if dice >= 0.75 {
        return true;
    }

    // Anchored prefix containment (e.g. `DJ Koze Remix` == `DJ Koze Pink Moon Remix`) when
    // neither side introduces a collaborator joiner (`Bicep & Hammer` != `Bicep`).
    if ja || jb || a_core.len() == b_core.len() {
        return false;
    }
    let (shorter, longer) = if a_core.len() < b_core.len() {
        (a_core, b_core)
    } else {
        (b_core, a_core)
    };
    !shorter.is_empty()
        && shorter
            .iter()
            .zip(longer.iter())
            .all(|(s, l)| tokens_match_remix(s, l))
}

/// Returns true when `candidate_mix` belongs to the same mix family as `expected_mix`
/// (always true if `expected_mix` is Original/empty so non-remixed queries keep matching).
pub fn mix_family_matches(expected_mix: Option<&str>, candidate_mix: Option<&str>) -> bool {
    let expected = classify_mix(expected_mix);
    if expected == MixFamily::Original {
        return true;
    }
    let cand = classify_mix(candidate_mix);
    match (&expected, &cand) {
        (
            MixFamily::NamedRemix {
                tokens: a,
                qualifiers: qa,
                has_joiner: ja,
            },
            MixFamily::NamedRemix {
                tokens: b,
                qualifiers: qb,
                has_joiner: jb,
            },
        ) => !qualifiers_conflict(qa, qb) && named_remix_tokens_match(a, *ja, b, *jb),
        _ => expected == cand,
    }
}

/// Search hint derived from a non-original mix name to sharpen provider queries.
pub fn mix_search_hint(mix: Option<&str>) -> Option<String> {
    match classify_mix(mix) {
        MixFamily::Original => None,
        MixFamily::Extended => Some("extended".to_owned()),
        MixFamily::Radio => Some("radio".to_owned()),
        MixFamily::Dub => Some("dub".to_owned()),
        MixFamily::Instrumental => Some("instrumental".to_owned()),
        MixFamily::Acapella => Some("acapella".to_owned()),
        MixFamily::Vip => Some("vip".to_owned()),
        MixFamily::NamedRemix { tokens, .. } => {
            let picked: Vec<String> = strip_leading_stopwords(&tokens)
                .iter()
                .filter(|tok| tok.as_str() != "and")
                .take(2)
                .cloned()
                .collect();
            if !picked.is_empty() {
                Some(picked.join(" "))
            } else {
                tokens.into_iter().next()
            }
        }
    }
}

fn remixer_matches_our_file(known: &KnownTags, remixer_tokens: &[String]) -> bool {
    let candidate_remixer = remixer_tokens.join(" ");
    if known
        .remixer
        .as_deref()
        .is_some_and(|ours| similarity(ours, &candidate_remixer) >= 0.65)
    {
        return true;
    }
    known
        .artist
        .as_deref()
        .is_some_and(|ours| similarity(ours, &candidate_remixer) >= 0.65)
}

fn title_similarity(known_title: Option<&str>, candidate_title: Option<&str>) -> Option<f64> {
    known_title.zip(candidate_title).map(|(a, b)| {
        let direct = similarity(a, b);
        let ca = search_title_clean(a);
        let cb = search_title_clean(b);
        if ca != a || cb != b {
            direct.max(similarity(&ca, &cb) * 0.96)
        } else {
            direct
        }
    })
}

fn artist_similarity(known_artist: Option<&str>, candidate_artist: Option<&str>) -> Option<f64> {
    known_artist.zip(candidate_artist).map(|(a, b)| {
        let direct = similarity(a, b);
        let (pa, fa) = split_featured(a);
        let (pb, fb) = split_featured(b);
        if fa.is_some() || fb.is_some() {
            direct.max(similarity(&pa, &pb) * 0.96)
        } else {
            direct
        }
    })
}

fn text_score_from_sims(title: Option<f64>, artist: Option<f64>) -> Option<f64> {
    match (title, artist) {
        (Some(title), Some(artist)) => Some(0.6 * title + 0.4 * artist),
        (Some(title), None) => Some(title),
        (None, Some(artist)) => Some(0.5 * artist),
        (None, None) => None,
    }
}

fn release_score(
    known: &KnownTags,
    candidate: &Candidate,
    catno_kind: CatnoMatch,
    label_same: bool,
    isrc_matched: bool,
) -> Option<f64> {
    if isrc_matched {
        return Some(1.0);
    }
    let catno = known
        .catno
        .as_ref()
        .zip(candidate.catno.as_ref())
        .map(|_| catno_kind);
    let label = known
        .label
        .as_deref()
        .zip(candidate.label.as_deref())
        .filter(|(_, candidate_label)| !is_generic_distributor_label(candidate_label))
        .map(|_| label_same);
    match (catno, label) {
        (Some(CatnoMatch::Exact), _) => Some(1.0),
        (Some(CatnoMatch::SameFamily), Some(true)) => Some(0.9),
        (Some(CatnoMatch::SameFamily), _) => Some(0.75),
        (Some(CatnoMatch::Different), Some(true)) => Some(0.5),
        (Some(CatnoMatch::Different), _) => Some(0.0),
        (None, Some(true)) => Some(0.8),
        (None, Some(false)) => Some(0.2),
        (None, None) => None,
    }
}

fn matches_isrc(ours: Option<&str>, theirs: Option<&str>) -> bool {
    let (Some(a), Some(b)) = (ours, theirs) else {
        return false;
    };
    catno_key(a) == catno_key(b) && !a.trim().is_empty()
}

fn catno_match(ours: Option<&str>, theirs: Option<&str>) -> CatnoMatch {
    let (Some(a), Some(b)) = (ours, theirs) else {
        return CatnoMatch::Different;
    };
    let ka = catno_key(a);
    let kb = catno_key(b);
    if ka.is_empty() || kb.is_empty() {
        return CatnoMatch::Different;
    }
    if ka == kb {
        return CatnoMatch::Exact;
    }
    if catno_stem(&ka) == catno_stem(&kb) && !catno_stem(&ka).is_empty() {
        return CatnoMatch::SameFamily;
    }
    CatnoMatch::Different
}

fn catno_stem(key: &str) -> &str {
    // Strips trailing digital format letters/numbers such as `DFTD412D2` -> `DFTD412`.
    let trimmed = key.trim_end_matches(['D', 'V', '1', '2']);
    if trimmed.len() >= 4 && trimmed.chars().any(|c| c.is_ascii_digit()) {
        trimmed
    } else {
        key
    }
}

fn labels_match(ours: Option<&str>, theirs: Option<&str>) -> bool {
    let (Some(a), Some(b)) = (ours, theirs) else {
        return false;
    };
    if is_generic_distributor_label(b) {
        return false;
    }
    let na = normalise_label(a);
    let nb = normalise_label(b);
    similarity(&na, &nb) >= 0.8 || similarity(a, b) >= 0.8
}

fn catno_key(catno: &str) -> String {
    catno
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> KnownTags {
        KnownTags {
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            ..KnownTags::default()
        }
    }

    fn candidate(title: &str, duration_ms: u64) -> Candidate {
        Candidate {
            provider: "test".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some(title.to_owned()),
            duration_ms: Some(duration_ms),
            ..Candidate::default()
        }
    }

    #[test]
    fn a_radio_edit_never_matches_an_extended_mix() {
        let extended_track_ms = 412_000;
        let ranked = rank(
            &known(),
            Some(extended_track_ms),
            vec![
                candidate("Night Drive (Radio Edit)", 201_000),
                candidate("Night Drive (Extended Mix)", 411_200),
            ],
        );
        assert_eq!(ranked[0].candidate.mix, None);
        assert_eq!(
            ranked[0].candidate.title.as_deref(),
            Some("Night Drive (Extended Mix)")
        );
        assert!(ranked[0].confidence >= crate::identify::STOP_THRESHOLD);
        assert!(ranked[1].confidence <= DIFFERENT_VERSION_CAP);

        let mut fingerprinted = candidate("Night Drive (Radio Edit)", 201_000);
        fingerprinted.fingerprint_score = Some(0.99);
        let ranked = score(&known(), Some(extended_track_ms), fingerprinted);
        assert!(ranked.confidence <= DIFFERENT_VERSION_CAP);
    }

    #[test]
    fn mix_names_that_disagree_are_different_versions_even_at_the_same_length() {
        let mut ours = known();
        ours.mix = Some("Extended Mix".to_owned());
        let ranked = score(
            &ours,
            Some(300_000),
            candidate("Night Drive (Dub)", 300_500),
        );
        assert!(ranked.confidence <= DIFFERENT_VERSION_CAP);
        let original = score(
            &KnownTags {
                mix: None,
                ..known()
            },
            Some(300_000),
            candidate("Night Drive (Original Mix)", 300_500),
        );
        assert!(original.confidence >= crate::identify::STOP_THRESHOLD);
    }

    #[test]
    fn the_audio_matches_only_when_chromaprint_says_so() {
        let text_only = score(&known(), Some(300_000), candidate("Night Drive", 300_000));
        assert!(!text_only.audio_matched);
        let mut weak = candidate("Night Drive", 300_000);
        weak.fingerprint_score = Some(0.4);
        assert!(!score(&known(), Some(300_000), weak).audio_matched);
        let mut strong = candidate("Night Drive", 300_000);
        strong.fingerprint_score = Some(0.95);
        let strong = score(&known(), Some(300_000), strong);
        assert!(strong.audio_matched);
        assert!(strong.confidence >= crate::identify::STOP_THRESHOLD);
    }

    #[test]
    fn catalogue_agreement_lifts_a_candidate_and_disagreement_lowers_it() {
        let mut ours = known();
        ours.catno = Some("DEF-023".to_owned());
        let mut same = candidate("Night Drive", 300_000);
        same.catno = Some("DEF023".to_owned());
        let mut other = candidate("Night Drive", 300_000);
        other.catno = Some("XYZ001".to_owned());
        let ranked = rank(&ours, Some(300_000), vec![other, same]);
        assert_eq!(ranked[0].candidate.catno.as_deref(), Some("DEF023"));
        assert!(ranked[0].confidence > ranked[1].confidence + 0.2);
    }

    #[test]
    fn text_candidates_are_confirmed_by_the_fingerprint_lookup() {
        let mut found = candidate("Night Drive", 300_000);
        found.recording_id = Some("rec-1".to_owned());
        let mut fingerprinted = Candidate {
            recording_id: Some("rec-1".to_owned()),
            fingerprint_score: Some(0.9),
            ..Candidate::default()
        };
        let mut candidates = vec![found];
        confirm_by_fingerprint(&mut candidates, std::slice::from_ref(&fingerprinted));
        assert_eq!(candidates[0].fingerprint_score, Some(0.9));
        fingerprinted.recording_id = Some("rec-2".to_owned());
        let mut unrelated = vec![candidate("Night Drive", 300_000)];
        confirm_by_fingerprint(&mut unrelated, &[fingerprinted]);
        assert_eq!(unrelated[0].fingerprint_score, None);
    }

    #[test]
    fn a_pasted_release_is_trusted_unless_the_audio_disagrees() {
        let mut pasted = candidate("Something Else", 300_000);
        pasted.user_supplied = true;
        assert!(score(&known(), Some(300_000), pasted.clone()).confidence >= USER_URL_CONFIDENCE);
        pasted.duration_ms = Some(180_000);
        assert!(score(&known(), Some(300_000), pasted).confidence <= DIFFERENT_VERSION_CAP);
    }

    #[test]
    fn named_remix_rejects_different_djs_qualifiers_and_extra_collaborators() {
        let mut ours = known();
        ours.mix = Some("Carl Cox Remix".to_owned());
        let craig = score(
            &ours,
            Some(360_000),
            candidate("Night Drive (Carl Craig Remix)", 360_000),
        );
        assert!(craig.confidence <= DIFFERENT_VERSION_CAP);

        ours.mix = Some("Vocal Mix".to_owned());
        let dub = score(
            &ours,
            Some(360_000),
            candidate("Night Drive (Dub Mix)", 360_000),
        );
        assert!(dub.confidence <= DIFFERENT_VERSION_CAP);

        ours.mix = Some("Bicep Remix".to_owned());
        let duo = score(
            &ours,
            Some(360_000),
            candidate("Night Drive (Bicep & Hammer Remix)", 360_000),
        );
        assert!(duo.confidence <= DIFFERENT_VERSION_CAP);

        ours.mix = Some("DJ Koze Remix".to_owned());
        let koze = score(
            &ours,
            Some(360_000),
            candidate("Night Drive (DJ Koze Pink Moon Remix)", 360_000),
        );
        assert!(koze.confidence >= crate::identify::STOP_THRESHOLD);
    }

    #[test]
    fn one_sided_remix_never_unlocks_using_candidates_artist_name() {
        let ours = KnownTags {
            artist: Some("Original Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            mix: None,
            ..KnownTags::default()
        };
        let remix_cand = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Guest DJ".to_owned()),
            title: Some("Night Drive".to_owned()),
            mix: Some("Guest DJ Remix".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        let ranked = score(&ours, Some(360_000), remix_cand);
        assert!(ranked.confidence <= ONE_SIDED_SIGNIFICANT_CAP);
    }

    #[test]
    fn remix_spellings_featured_artists_and_possessives_align_cleanly() {
        let ours = KnownTags {
            artist: Some("Clara".to_owned()),
            title: Some("Night Drive".to_owned()),
            mix: Some("Morgans Remix".to_owned()),
            ..KnownTags::default()
        };
        let cand = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Clara feat. Guest".to_owned()),
            title: Some("Night Drive".to_owned()),
            mix: Some("Morgan Remix".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        let ranked = score(&ours, Some(360_000), cand);
        assert!(ranked.confidence >= crate::identify::STOP_THRESHOLD);

        assert!(mix_family_matches(
            Some("Devon King's Deep Mix"),
            Some("Devon King Remix")
        ));
    }

    #[test]
    fn duration_verified_candidate_breaks_equal_confidence_ties_over_undated_single() {
        let known = KnownTags {
            title: Some("Midnight Signal".to_owned()),
            ..KnownTags::default()
        };
        let undated_single = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Other Artist".to_owned()),
            title: Some("Midnight Signal".to_owned()),
            release_kind: crate::identify::ReleaseKind::SingleOrEp,
            ..Candidate::default()
        };
        let duration_matched_album = Candidate {
            provider: "musicbrainz".to_owned(),
            artist: Some("Unknown".to_owned()),
            title: Some("Midnight Signal".to_owned()),
            duration_ms: Some(361_900),
            release_kind: crate::identify::ReleaseKind::Compilation,
            ..Candidate::default()
        };

        let ranked = rank(
            &known,
            Some(360_000),
            vec![undated_single, duration_matched_album],
        );
        assert_eq!(ranked[0].confidence, MISSING_ARTIST_CAP);
        assert_eq!(ranked[1].confidence, MISSING_ARTIST_CAP);
        assert_eq!(ranked[0].candidate.provider, "musicbrainz");

        let single_with_dur = Candidate {
            provider: "deezer".to_owned(),
            artist: Some("Synth Lab".to_owned()),
            title: Some("Midnight Signal".to_owned()),
            duration_ms: Some(360_500),
            release_kind: crate::identify::ReleaseKind::SingleOrEp,
            ..Candidate::default()
        };
        let compilation_with_dur = Candidate {
            provider: "musicbrainz".to_owned(),
            artist: Some("Synth Lab".to_owned()),
            title: Some("Midnight Signal".to_owned()),
            duration_ms: Some(360_200),
            release_kind: crate::identify::ReleaseKind::Compilation,
            ..Candidate::default()
        };
        let ranked_both = rank(
            &known,
            Some(360_000),
            vec![compilation_with_dur, single_with_dur],
        );
        assert_eq!(ranked_both[0].candidate.provider, "deezer");
    }

    #[test]
    fn remix_qualifiers_stopwords_and_fuzzy_lookalikes_do_not_false_match() {
        assert!(!mix_family_matches(
            Some("Artist Remix"),
            Some("Artist Dub")
        ));
        assert!(!mix_family_matches(
            Some("Artist Remix"),
            Some("Artist Instrumental")
        ));
        assert!(!mix_family_matches(
            Some("Artist Remix"),
            Some("Artist Radio Edit")
        ));
        assert!(!mix_family_matches(
            Some("Artist Remix"),
            Some("Artist Acapella")
        ));
        assert!(!mix_family_matches(
            Some("Light Remix"),
            Some("Night Remix")
        ));
        assert!(!mix_family_matches(
            Some("Steve Remix"),
            Some("Stevo Remix")
        ));
        assert!(!mix_family_matches(
            Some("Part1 Remix"),
            Some("Part2 Remix")
        ));
        assert!(!mix_family_matches(
            Some("Light Night Remix"),
            Some("Night Remix")
        ));
        assert!(!mix_family_matches(
            Some("The Remix"),
            Some("The Blessed Madonna Remix")
        ));
        assert_eq!(
            mix_search_hint(Some("The Koze Remix")).as_deref(),
            Some("koze")
        );

        let known_plain = KnownTags {
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            ..KnownTags::default()
        };
        let radio = Candidate {
            provider: "deezer".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            mix: Some("Radio Edit".to_owned()),
            duration_ms: Some(195_000),
            ..Candidate::default()
        };
        let ranked = score(&known_plain, None, radio);
        assert!(ranked.confidence <= ONE_SIDED_SIGNIFICANT_CAP);
    }

    #[test]
    fn different_title_on_same_artist_and_catno_is_capped_at_zero_text() {
        let ours = KnownTags {
            artist: Some("Artist A".to_owned()),
            title: Some("First Track".to_owned()),
            catno: Some("EP001".to_owned()),
            ..KnownTags::default()
        };
        let b_side = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Artist A".to_owned()),
            title: Some("Completely Different B-Side".to_owned()),
            catno: Some("EP001".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        let ranked = score(&ours, Some(360_000), b_side);
        assert!(ranked.confidence <= ZERO_TEXT_CAP);
    }

    #[test]
    fn named_edit_rejects_original_release_unless_remixer_is_credited_and_zero_artist_blocks_wrong_artist()
     {
        let mut ours = known();
        ours.mix = Some("Will & Nova Edit".to_owned());

        let original_by_orig_artist = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        assert!(
            score(&ours, Some(360_000), original_by_orig_artist).confidence
                <= DIFFERENT_VERSION_CAP
        );

        let credited_to_remixer = Candidate {
            provider: "bandcamp".to_owned(),
            artist: Some("Will & Nova".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        assert!(score(&ours, Some(360_000), credited_to_remixer).confidence > 0.85);

        let mut dub_known = known();
        dub_known.mix = Some("Dub Mix".to_owned());
        let plain = candidate("Night Drive", 360_000);
        assert!(score(&dub_known, Some(360_000), plain).confidence <= DIFFERENT_VERSION_CAP);

        let wrong_artist = Candidate {
            provider: "itunes".to_owned(),
            artist: Some("Completely Unrelated".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        assert!(score(&known(), Some(360_000), wrong_artist).confidence <= ZERO_TEXT_CAP);

        let wrong_artist_on_remixer_album = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Completely Unrelated".to_owned()),
            album: Some("Will & Nova Edits Vol. 1".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        assert!(
            score(&ours, Some(360_000), wrong_artist_on_remixer_album).confidence <= ZERO_TEXT_CAP
        );

        let mut deep_mix = known();
        deep_mix.mix = Some("Deep Mix".to_owned());
        let deep_classics_original = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            album: Some("Deep House Classics Vol. 4".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        assert!(
            score(&deep_mix, Some(360_000), deep_classics_original).confidence
                <= DIFFERENT_VERSION_CAP
        );

        let mut love_edit = known();
        love_edit.mix = Some("Love Edit".to_owned());
        let love_machine_artist = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Love Machine".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        assert!(score(&love_edit, Some(360_000), love_machine_artist).confidence <= ZERO_TEXT_CAP);

        let mut bicep_remix = known();
        bicep_remix.mix = Some("Bicep Remix".to_owned());
        let bicep_artist = Candidate {
            provider: "bandcamp".to_owned(),
            artist: Some("Bicep".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: Some(360_000),
            ..Candidate::default()
        };
        assert!(score(&bicep_remix, Some(360_000), bicep_artist).confidence > 0.85);

        assert_eq!(
            mix_search_hint(Some("Will & Nova Edit")).as_deref(),
            Some("will nova")
        );
        assert_eq!(
            mix_search_hint(Some("Velvet Pulse Remix")).as_deref(),
            Some("velvet pulse")
        );
    }
}
