//! Cross-provider clustering, field fusion, consensus boosting and evidence summary.

use std::collections::HashSet;

use crate::library::TagField;

use super::parse::{is_generic_distributor_label, similarity, split_mix};
use super::score::{MixFamily, ScoreGuards, classify_mix, compare_ranked, score_with_guards};
use super::{
    AudioMatchSource, Candidate, EMBEDDABLE_ARTWORK_PROVIDERS, FINGERPRINT_MATCH_SCORE, KnownTags,
    MatchBasis, MatchEvidence, PREVIEW_MATCH_SCORE, RankedCandidate, ReleaseKind, STOP_THRESHOLD,
};

/// Network lineage so AcoustID + MusicBrainz never double-count as independent consensus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderLineage {
    MusicBrainz,
    Discogs,
    Deezer,
    Itunes,
    Bandcamp,
    SoundCloud,
    YouTube,
    AudD,
    Other,
}

impl ProviderLineage {
    pub fn of(provider: &str) -> Self {
        match provider {
            "acoustid" | "musicbrainz" => Self::MusicBrainz,
            "discogs" => Self::Discogs,
            "deezer" => Self::Deezer,
            "itunes" => Self::Itunes,
            "bandcamp" => Self::Bandcamp,
            "soundcloud" => Self::SoundCloud,
            "youtube" => Self::YouTube,
            "audd" => Self::AudD,
            _ => Self::Other,
        }
    }
}

struct ScoredItem {
    ranked: RankedCandidate,
    guards: ScoreGuards,
    mix_family: MixFamily,
}

/// Clusters matching recordings across providers, merges the richest legal metadata per field,
/// applies independent cross-provider consensus, and enforces every safety cap afterwards.
pub fn fuse_and_rank(
    known: &KnownTags,
    duration_ms: Option<u64>,
    candidates: Vec<Candidate>,
) -> Vec<RankedCandidate> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let scored: Vec<ScoredItem> = candidates
        .into_iter()
        .map(|candidate| {
            let mix_text = candidate.mix.clone().or_else(|| {
                candidate
                    .title
                    .as_deref()
                    .and_then(|title| split_mix(title).1)
            });
            let mix_family = classify_mix(mix_text.as_deref());
            let (ranked, guards) = score_with_guards(known, duration_ms, candidate);
            ScoredItem {
                ranked,
                guards,
                mix_family,
            }
        })
        .collect();

    let mut clusters: Vec<Vec<ScoredItem>> = Vec::new();
    for item in scored {
        if let Some(cluster) = clusters.iter_mut().find(|c| belongs_to_cluster(c, &item)) {
            cluster.push(item);
        } else {
            clusters.push(vec![item]);
        }
    }

    let mut fused: Vec<RankedCandidate> = clusters
        .into_iter()
        .map(|cluster| merge_cluster(known, duration_ms, cluster))
        .collect();
    fused.sort_by(compare_ranked);
    fused
}

fn belongs_to_cluster(cluster: &[ScoredItem], item: &ScoredItem) -> bool {
    let Some(anchor) = cluster.first() else {
        return false;
    };
    if anchor.guards.different_version != item.guards.different_version {
        return false;
    }
    let a = &anchor.ranked.candidate;
    let b = &item.ranked.candidate;

    let durations_close = match (a.duration_ms, b.duration_ms) {
        (Some(da), Some(db)) => da.abs_diff(db) <= 4_000,
        _ => anchor.mix_family == item.mix_family,
    };
    if !durations_close {
        return false;
    }

    if let (Some(ia), Some(ib)) = (&a.isrc, &b.isrc)
        && ia.eq_ignore_ascii_case(ib)
    {
        return true;
    }
    if let (Some(ra), Some(rb)) = (&a.recording_id, &b.recording_id)
        && ra == rb
    {
        return true;
    }

    let title_a = effective_title(a);
    let title_b = effective_title(b);
    let artist_a = a.artist.as_deref();
    let artist_b = b.artist.as_deref();
    match ((title_a, title_b), (artist_a, artist_b)) {
        ((Some(ta), Some(tb)), (Some(aa), Some(ab))) => {
            similarity(ta, tb) >= 0.85 && similarity(aa, ab) >= 0.80
        }
        ((Some(ta), Some(tb)), _) if anchor.mix_family == item.mix_family => {
            similarity(ta, tb) >= 0.90 && a.artist == b.artist
        }
        _ => false,
    }
}

fn effective_title(candidate: &Candidate) -> Option<&str> {
    candidate.title.as_deref()
}

fn merge_cluster(
    known: &KnownTags,
    duration_ms: Option<u64>,
    mut cluster: Vec<ScoredItem>,
) -> RankedCandidate {
    cluster.sort_by(|a, b| compare_ranked(&a.ranked, &b.ranked));

    let mut providers: Vec<String> = Vec::new();
    let mut consensus_lineages: HashSet<ProviderLineage> = HashSet::new();
    let mut combined_guards = cluster[0].guards;

    for item in &cluster {
        if !providers.contains(&item.ranked.candidate.provider) {
            providers.push(item.ranked.candidate.provider.clone());
        }
        combined_guards.merge_from(item.guards);
        let eligible = !item.guards.different_version
            && (item.ranked.audio_matched
                || item.guards.text_similarity >= 0.70
                || item.guards.isrc_matched);
        if eligible {
            consensus_lineages.insert(ProviderLineage::of(&item.ranked.candidate.provider));
        }
    }

    let best_item = &cluster[0];
    let mut merged = best_item.ranked.candidate.clone();
    let mut field_providers: Vec<(TagField, String)> = Vec::new();

    let pick_text = |get: fn(&Candidate) -> Option<&String>, priority: &[&str]| {
        for &pref in priority {
            if let Some(item) = cluster.iter().find(|it| {
                it.ranked.candidate.provider == pref && get(&it.ranked.candidate).is_some()
            }) && let Some(val) = get(&item.ranked.candidate)
            {
                return Some((val.clone(), item.ranked.candidate.provider.clone()));
            }
        }
        cluster.iter().find_map(|it| {
            get(&it.ranked.candidate).map(|v| (v.clone(), it.ranked.candidate.provider.clone()))
        })
    };

    if let Some((artist, prov)) = pick_text(
        |c| c.artist.as_ref(),
        &[
            "discogs",
            "musicbrainz",
            "bandcamp",
            "deezer",
            "itunes",
            "acoustid",
        ],
    ) {
        merged.artist = Some(artist);
        field_providers.push((TagField::Artist, prov));
    }
    let mut extracted_title_mix: Option<(String, String)> = None;
    if let Some((title, prov)) = pick_text(
        |c| c.title.as_ref(),
        &[
            "discogs",
            "musicbrainz",
            "bandcamp",
            "deezer",
            "itunes",
            "acoustid",
        ],
    ) {
        let (clean_title, extracted_mix) = split_mix(&title);
        merged.title = Some(clean_title);
        if let Some(mix) = extracted_mix {
            extracted_title_mix = Some((mix, prov.clone()));
        }
        field_providers.push((TagField::Title, prov));
    }
    if let Some((mix, prov)) = pick_text(
        |c| c.mix.as_ref(),
        &["discogs", "bandcamp", "deezer", "musicbrainz", "itunes"],
    ) {
        merged.mix = Some(mix);
        field_providers.push((TagField::Mix, prov));
    } else if let Some((mix, prov)) = extracted_title_mix {
        merged.mix = Some(mix);
        field_providers.push((TagField::Mix, prov));
    }
    const ALBUM_PRIORITY: [&str; 5] = ["discogs", "musicbrainz", "bandcamp", "deezer", "itunes"];
    let non_comp_album = ALBUM_PRIORITY.iter().find_map(|&pref| {
        cluster
            .iter()
            .find(|it| {
                it.ranked.candidate.provider == pref
                    && it.ranked.candidate.release_kind != ReleaseKind::Compilation
                    && it.ranked.candidate.album.is_some()
            })
            .and_then(|it| {
                it.ranked
                    .candidate
                    .album
                    .as_ref()
                    .map(|val| (val.clone(), pref.to_owned()))
            })
    });
    if let Some((album, prov)) =
        non_comp_album.or_else(|| pick_text(|c| c.album.as_ref(), &ALBUM_PRIORITY))
    {
        merged.album = Some(album);
        field_providers.push((TagField::Album, prov));
    }

    // Label prefers real labels from release databases over generic distributor strings.
    const LABEL_PRIORITY: [&str; 5] = ["discogs", "musicbrainz", "bandcamp", "deezer", "itunes"];
    let real_label = LABEL_PRIORITY.iter().find_map(|&pref| {
        cluster.iter().find_map(|it| {
            if it.ranked.candidate.provider != pref {
                return None;
            }
            let label = it.ranked.candidate.label.as_deref()?;
            (!is_generic_distributor_label(label)).then(|| (label.to_owned(), pref.to_owned()))
        })
    });
    if let Some((label, prov)) =
        real_label.or_else(|| pick_text(|c| c.label.as_ref(), &LABEL_PRIORITY))
    {
        merged.label = Some(label);
        field_providers.push((TagField::Label, prov));
    }

    if let Some((catno, prov)) = pick_text(
        |c| c.catno.as_ref(),
        &["discogs", "musicbrainz", "bandcamp"],
    ) {
        merged.catno = Some(catno);
        field_providers.push((TagField::CatNo, prov));
    }
    if let Some((genre, prov)) = pick_text(
        |c| c.genre.as_ref(),
        &["discogs", "bandcamp", "deezer", "itunes"],
    ) {
        merged.genre = Some(genre);
        field_providers.push((TagField::Genre, prov));
    }
    if let Some((subgenre, prov)) = pick_text(|c| c.subgenre.as_ref(), &["discogs", "bandcamp"]) {
        merged.subgenre = Some(subgenre);
        field_providers.push((TagField::Subgenre, prov));
    }
    if let Some((track_number, prov)) = pick_text(
        |c| c.track_number.as_ref(),
        &["discogs", "bandcamp", "deezer", "itunes"],
    ) {
        merged.track_number = Some(track_number);
        field_providers.push((TagField::TrackNumber, prov));
    }
    if let Some((isrc, _)) = pick_text(|c| c.isrc.as_ref(), &["deezer", "musicbrainz", "audd"]) {
        merged.isrc = Some(isrc);
    }

    if let Some((year, prov)) = pick_earliest_release_year(&cluster) {
        merged.year = Some(year);
        field_providers.push((TagField::Year, prov));
    }
    if let Some((url, _)) = pick_text(
        |c| c.url.as_ref(),
        &[
            "discogs",
            "bandcamp",
            "musicbrainz",
            "deezer",
            "itunes",
            "soundcloud",
            "youtube",
        ],
    ) {
        merged.url = Some(url);
    }

    // Only providers whose terms allow a cover to be embedded in a file.
    let (artwork_url, artwork_provider) = pick_embeddable_artwork(&cluster);
    merged.artwork_url = artwork_url;
    merged.artwork_provider = artwork_provider;

    // Audio confirmation propagates only within this same-recording cluster when safe.
    for item in &cluster {
        let c = &item.ranked.candidate;
        if c.fingerprint_score
            .is_some_and(|score| score >= FINGERPRINT_MATCH_SCORE)
            && merged
                .fingerprint_score
                .is_none_or(|cur| c.fingerprint_score > Some(cur))
        {
            merged.fingerprint_score = c.fingerprint_score;
            merged
                .audio_source
                .get_or_insert(AudioMatchSource::Fingerprint);
        }
        if !item.guards.different_version
            && c.preview_similarity
                .is_some_and(|sim| sim >= PREVIEW_MATCH_SCORE)
            && merged
                .preview_similarity
                .is_none_or(|cur| c.preview_similarity > Some(cur))
        {
            merged.preview_similarity = c.preview_similarity;
            if merged.audio_source.is_none() {
                merged.audio_source = Some(AudioMatchSource::PreviewMatch);
            }
        }
        if !item.guards.different_version
            && c.audio_source == Some(AudioMatchSource::Recogniser)
            && merged.audio_source.is_none()
        {
            merged.audio_source = Some(AudioMatchSource::Recogniser);
        }
        if merged.preview_url.is_none() && c.preview_url.is_some() {
            merged.preview_url.clone_from(&c.preview_url);
        }
        if merged.recording_id.is_none() && c.recording_id.is_some() {
            merged.recording_id.clone_from(&c.recording_id);
        }
        if merged.duration_ms.is_none()
            && let Some(sib_dur) = c.duration_ms
        {
            merged.duration_ms = Some(sib_dur);
        }
        if merged.release_kind == ReleaseKind::Unknown && c.release_kind != ReleaseKind::Unknown {
            merged.release_kind = c.release_kind;
        }
        merged.user_supplied |= c.user_supplied;
        merged.from_embedded_url |= c.from_embedded_url;
    }

    if let Some(target_ms) = duration_ms {
        let best_sib_dur = cluster
            .iter()
            .filter_map(|it| it.ranked.candidate.duration_ms)
            .min_by_key(|&d| d.abs_diff(target_ms));
        if best_sib_dur.is_some() {
            merged.duration_ms = best_sib_dur;
        }
    }

    merged.field_providers = field_providers;

    let (mut ranked, rescored_guards) = score_with_guards(known, duration_ms, merged);
    let mut final_guards = combined_guards;
    final_guards.merge_from(rescored_guards);
    final_guards.different_version =
        rescored_guards.different_version || cluster[0].guards.different_version;

    // Cross-provider independent consensus boost, capped BEFORE re-applying hard safety caps,
    // without ever downgrading a cluster that already scored above 0.97.
    let consensus_bonus = match consensus_lineages.len() {
        0 | 1 => 0.0,
        2 => 0.06,
        _ => 0.09,
    };
    if consensus_bonus > 0.0 && !final_guards.different_version {
        ranked.confidence = ranked
            .confidence
            .max((ranked.confidence + consensus_bonus).min(0.97));
    }
    ranked.confidence = final_guards.apply_caps(ranked.confidence, ranked.candidate.user_supplied);
    ranked.providers = providers.clone();
    ranked.evidence = build_match_evidence(&ranked, providers);
    ranked
}

fn pick_earliest_release_year(cluster: &[ScoredItem]) -> Option<(u16, String)> {
    cluster
        .iter()
        .filter_map(|it| {
            let year = it
                .ranked
                .candidate
                .year
                .filter(|y| (1950..=2100).contains(y))?;
            let is_comp = u8::from(it.ranked.candidate.release_kind == ReleaseKind::Compilation);
            Some((is_comp, year, it.ranked.candidate.provider.clone()))
        })
        .min_by_key(|&(is_comp, year, _)| (is_comp, year))
        .map(|(_, year, provider)| (year, provider))
}

fn pick_embeddable_artwork(cluster: &[ScoredItem]) -> (Option<String>, Option<String>) {
    for &pref in &EMBEDDABLE_ARTWORK_PROVIDERS {
        if let Some(item) = cluster.iter().find(|it| {
            it.ranked.candidate.artwork_url.is_some()
                && it
                    .ranked
                    .candidate
                    .artwork_provider
                    .as_deref()
                    .unwrap_or(it.ranked.candidate.provider.as_str())
                    == pref
        }) {
            let url = item.ranked.candidate.artwork_url.clone();
            return (url, Some(pref.to_owned()));
        }
    }
    (None, None)
}

pub fn build_match_evidence(ranked: &RankedCandidate, providers: Vec<String>) -> MatchEvidence {
    let c = &ranked.candidate;
    let basis = super::score::classify_match_basis(
        ranked.audio_matched,
        c.audio_source,
        c.user_supplied,
        c.from_embedded_url,
        ranked.evidence.catno_matched,
        ranked.duration_delta_ms,
    );
    MatchEvidence {
        basis,
        providers,
        primary_provider: c.provider.clone(),
        duration_delta_ms: ranked.duration_delta_ms,
        preview_similarity: c.preview_similarity,
        catno_matched: ranked.evidence.catno_matched,
        isrc_matched: ranked.evidence.isrc_matched,
        url: c.url.clone(),
    }
}

/// Minimum confidence for any identification result to be kept instead of treating the track
/// as unresolved (`NeedsUrl`).
pub const MIN_VIABLE_CONFIDENCE: f64 = 0.45;

/// Minimum confidence for an unverified `TagsOnly` result (no audio, catno, or ISRC confirmation).
pub const MIN_TAGS_ONLY_CONFIDENCE: f64 = 0.70;

/// Minimum confidence for an alternative match shown in Prep.
pub const MIN_ALTERNATIVE_CONFIDENCE: f64 = 0.55;

/// Minimum confidence for an alternative match when the selected top match is already confident.
pub const MIN_HIGH_CONF_ALTERNATIVE: f64 = 0.65;

/// Whether a ranked candidate is strong enough to stage as an identification result.
pub fn is_viable_candidate(candidate: &RankedCandidate) -> bool {
    if candidate.candidate.user_supplied {
        return true;
    }
    if candidate.evidence.basis == MatchBasis::TagsOnly
        && !candidate.audio_matched
        && !candidate.evidence.catno_matched
        && !candidate.evidence.isrc_matched
    {
        return candidate.confidence >= MIN_TAGS_ONLY_CONFIDENCE;
    }
    candidate.confidence >= MIN_VIABLE_CONFIDENCE
}

/// A candidate this much longer or shorter than the file is another edit of the same recording,
/// so its audio matching the file's says nothing about which edit the file is.
pub const EDIT_LENGTH_GAP_MS: u64 = 25_000;

/// The stored fingerprint covers the first 120 seconds. A preview cut from later in a longer
/// file is only found by fingerprinting the whole file.
pub const FINGERPRINT_PREFIX_MS: u64 = 125_000;

/// The mix searched for when a preview matched a different edit of the file. Stores sell the
/// radio edit and the extended mix of a club track; the file is the longer one.
pub const LONGER_EDIT_MIX: &str = "Extended Mix";

/// Only a match this strong is worth looking up again by its ISRC in another catalogue.
pub const ENRICHMENT_THRESHOLD: f64 = 0.80;

/// Whether a candidate is a different edit from the file, judged by length.
pub fn is_other_edit(file_ms: Option<u64>, candidate_ms: Option<u64>) -> bool {
    file_ms
        .zip(candidate_ms)
        .is_some_and(|(ours, theirs)| ours.abs_diff(theirs) > EDIT_LENGTH_GAP_MS)
}

/// Whether the best match still needs checking by ear: there is none, it is not confident, or
/// only one provider backs it on names alone.
pub fn needs_preview_check(best: Option<&RankedCandidate>, stop_threshold: f64) -> bool {
    let Some(best) = best else {
        return true;
    };
    !best.audio_matched
        && (best.confidence < stop_threshold
            || (best.evidence.basis != MatchBasis::CatalogAndDuration && best.providers.len() < 2))
}

/// Whether a newly ranked best should replace the current one. Confidence decides first; on a
/// tie, an audio match, a known length, more providers, a usable cover or a catalogue number win.
pub fn replaces_best(current: &RankedCandidate, best: &RankedCandidate) -> bool {
    if best.confidence != current.confidence {
        return best.confidence > current.confidence;
    }
    if best.audio_matched != current.audio_matched {
        return best.audio_matched;
    }
    if best.duration_delta_ms.is_some() != current.duration_delta_ms.is_some() {
        return best.duration_delta_ms.is_some();
    }
    let has_art = |item: &RankedCandidate| item.candidate.artwork_fetcher_provider().is_some();
    let has_catno = |item: &RankedCandidate| {
        item.candidate
            .field_providers
            .iter()
            .any(|(field, _)| *field == TagField::CatNo)
    };
    best.providers.len() > current.providers.len()
        || (has_art(best) && !has_art(current))
        || (!has_catno(current) && has_catno(best))
}
/// Whether two ranked answers name the same release track from the same provider, whatever
/// their scores.
pub fn is_same_candidate(a: &Candidate, b: &Candidate) -> bool {
    a.provider == b.provider
        && a.artist == b.artist
        && a.title == b.title
        && a.mix == b.mix
        && a.album == b.album
        && a.catno == b.catno
        && a.duration_ms == b.duration_ms
        && a.url == b.url
}
/// Whether `alt` is relevant alongside `top` without showing unrelated tracks from the same release.
pub fn is_viable_alternative(top: &RankedCandidate, alt: &RankedCandidate) -> bool {
    if alt.confidence < MIN_ALTERNATIVE_CONFIDENCE {
        return false;
    }
    if top.confidence >= STOP_THRESHOLD && alt.confidence < MIN_HIGH_CONF_ALTERNATIVE {
        return false;
    }
    if top.candidate.url.is_some() && alt.candidate.url == top.candidate.url {
        return top
            .candidate
            .title
            .as_deref()
            .zip(alt.candidate.title.as_deref())
            .is_some_and(|(a, b)| similarity(a, b) >= 0.70);
    }
    true
}

/// Keeps only viable identifications and relevant alternatives; drops low-confidence false
/// positives and unrelated sibling tracks from the same release.
pub fn retain_viable_candidates(ranked: Vec<RankedCandidate>) -> Vec<RankedCandidate> {
    let mut iter = ranked.into_iter();
    let Some(top) = iter.next() else {
        return Vec::new();
    };
    if !is_viable_candidate(&top) {
        return Vec::new();
    }
    let mut kept = vec![top];
    for alt in iter {
        if is_viable_alternative(&kept[0], &alt) {
            kept.push(alt);
        }
    }
    kept
}

/// Auto-expands the candidate list in Prep so any identified match (even a single match) is
/// visible at a glance along with alternatives that add cover art or a catalogue number.
pub fn should_expand_candidates(ranked: &[RankedCandidate]) -> bool {
    if ranked.is_empty() {
        return false;
    }
    if ranked.len() == 1 {
        return true;
    }
    let top = &ranked[0];
    let alts = &ranked[1..];
    let alt_adds_artwork = top.candidate.artwork_fetcher_provider().is_none()
        && alts
            .iter()
            .any(|alt| alt.candidate.artwork_fetcher_provider().is_some());
    let alt_adds_catno =
        top.candidate.catno.is_none() && alts.iter().any(|alt| alt.candidate.catno.is_some());
    top.confidence < STOP_THRESHOLD
        || (top.confidence - alts[0].confidence) < 0.08
        || alt_adds_artwork
        || alt_adds_catno
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::rank;
    use crate::identify::score::DIFFERENT_VERSION_CAP;

    #[test]
    fn consensus_never_overrides_version_or_artist_safety_caps() {
        let known = KnownTags {
            artist: Some("Artist".to_owned()),
            title: Some("Song".to_owned()),
            mix: Some("Extended Mix".to_owned()),
            ..KnownTags::default()
        };
        let radio = |provider: &str| Candidate {
            provider: provider.to_owned(),
            artist: Some("Artist".to_owned()),
            title: Some("Song".to_owned()),
            mix: Some("Radio Edit".to_owned()),
            duration_ms: Some(190_000),
            ..Candidate::default()
        };
        let ranked = fuse_and_rank(
            &known,
            Some(400_000),
            vec![radio("deezer"), radio("discogs"), radio("itunes")],
        );
        assert!(ranked[0].confidence <= DIFFERENT_VERSION_CAP);
        assert!(!ranked[0].audio_matched);
    }

    #[test]
    fn acoustid_and_musicbrainz_share_one_lineage_for_consensus() {
        let known = KnownTags {
            artist: Some("Artist".to_owned()),
            title: Some("Song".to_owned()),
            ..KnownTags::default()
        };
        let base = |provider: &str| Candidate {
            provider: provider.to_owned(),
            artist: Some("Artist".to_owned()),
            title: Some("Song".to_owned()),
            duration_ms: Some(306_000),
            ..Candidate::default()
        };
        let mb_pair = fuse_and_rank(
            &known,
            Some(300_000),
            vec![base("acoustid"), base("musicbrainz")],
        );
        let two_independent = fuse_and_rank(
            &known,
            Some(300_000),
            vec![base("musicbrainz"), base("deezer")],
        );
        assert!(two_independent[0].confidence > mb_pair[0].confidence + 0.04);
    }

    #[test]
    fn missing_artist_and_missing_duration_caps_hold_under_three_provider_consensus() {
        let no_artist = KnownTags {
            title: Some("Three Word Song".to_owned()),
            ..KnownTags::default()
        };
        let c = |provider: &str| Candidate {
            provider: provider.to_owned(),
            artist: Some("Someone".to_owned()),
            title: Some("Three Word Song".to_owned()),
            duration_ms: Some(300_000),
            ..Candidate::default()
        };
        let ranked = fuse_and_rank(
            &no_artist,
            Some(300_000),
            vec![c("deezer"), c("discogs"), c("itunes")],
        );
        assert!(ranked[0].confidence <= crate::identify::score::MISSING_ARTIST_CAP);

        let no_duration_known = KnownTags {
            artist: Some("Someone".to_owned()),
            title: Some("Three Word Song".to_owned()),
            ..KnownTags::default()
        };
        let no_dur = |provider: &str| Candidate {
            provider: provider.to_owned(),
            artist: Some("Someone".to_owned()),
            title: Some("Three Word Song".to_owned()),
            duration_ms: None,
            ..Candidate::default()
        };
        let ranked_no_dur = fuse_and_rank(
            &no_duration_known,
            Some(300_000),
            vec![no_dur("deezer"), no_dur("discogs"), no_dur("musicbrainz")],
        );
        assert!(ranked_no_dur[0].confidence <= crate::identify::score::MISSING_DURATION_CAP);
    }

    #[test]
    fn fusion_merges_legal_artwork_and_skips_distributor_labels() {
        let known = KnownTags {
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            ..KnownTags::default()
        };
        let deezer = Candidate {
            provider: "deezer".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            label: Some("DistroKid".to_owned()),
            duration_ms: Some(412_000),
            isrc: Some("USRC12345678".to_owned()),
            ..Candidate::default()
        };
        let discogs = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            label: Some("Defected".to_owned()),
            catno: Some("DFTD001".to_owned()),
            genre: Some("Electronic".to_owned()),
            subgenre: Some("House".to_owned()),
            year: Some(2020),
            duration_ms: Some(412_000),
            artwork_url: Some("https://i.discogs.com/cover.jpg".to_owned()),
            artwork_provider: Some("discogs".to_owned()),
            ..Candidate::default()
        };
        let mb = Candidate {
            provider: "musicbrainz".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: Some(412_000),
            year: Some(2019),
            release_kind: ReleaseKind::SingleOrEp,
            artwork_url: Some("https://coverartarchive.org/release/abc/front-500".to_owned()),
            artwork_provider: Some("musicbrainz".to_owned()),
            ..Candidate::default()
        };
        let ranked = fuse_and_rank(&known, Some(412_000), vec![deezer, discogs, mb]);
        assert_eq!(ranked.len(), 1);
        let best = &ranked[0].candidate;
        assert_eq!(best.label.as_deref(), Some("Defected"));
        assert_eq!(best.catno.as_deref(), Some("DFTD001"));
        assert_eq!(best.genre.as_deref(), Some("Electronic"));
        assert_eq!(best.subgenre.as_deref(), Some("House"));
        assert_eq!(best.year, Some(2019));
        assert_eq!(best.artwork_provider.as_deref(), Some("musicbrainz"));
        assert_eq!(best.provider_for(TagField::CatNo), "discogs");
        assert_eq!(best.provider_for(TagField::Year), "musicbrainz");
    }

    #[test]
    fn consensus_never_downgrades_high_confidence_cluster_and_strips_mb_mix_from_title() {
        let known = KnownTags {
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            mix: Some("Extended Mix".to_owned()),
            ..KnownTags::default()
        };
        let deezer = Candidate {
            provider: "deezer".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            mix: Some("Extended Mix".to_owned()),
            duration_ms: Some(412_000),
            isrc: Some("GBXYZ2100099".to_owned()),
            artwork_url: Some("https://cdn-images.dzcdn.net/cover.jpg".to_owned()),
            artwork_provider: None,
            ..Candidate::default()
        };
        let musicbrainz = Candidate {
            provider: "musicbrainz".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive (Extended Mix)".to_owned()),
            duration_ms: Some(412_000),
            isrc: Some("GBXYZ2100099".to_owned()),
            artwork_url: Some("https://coverartarchive.org/release/rel-1/front-500".to_owned()),
            artwork_provider: Some("musicbrainz".to_owned()),
            ..Candidate::default()
        };
        let ranked = fuse_and_rank(&known, Some(412_000), vec![deezer, musicbrainz]);
        assert_eq!(ranked.len(), 1);
        let best = &ranked[0];
        assert!(best.confidence >= 0.99);
        assert_eq!(best.candidate.title.as_deref(), Some("Night Drive"));
        assert_eq!(best.candidate.mix.as_deref(), Some("Extended Mix"));
        assert_eq!(
            best.candidate.artwork_provider.as_deref(),
            Some("musicbrainz")
        );
    }

    #[test]
    fn undated_match_never_merges_with_a_different_edit_duration() {
        let known = KnownTags {
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            ..KnownTags::default()
        };
        let discogs_undated = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: None,
            catno: Some("CAT001".to_owned()),
            ..Candidate::default()
        };
        let deezer_radio = Candidate {
            provider: "deezer".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            duration_ms: Some(195_000),
            ..Candidate::default()
        };
        let ranked = fuse_and_rank(&known, Some(391_000), vec![discogs_undated, deezer_radio]);
        assert_eq!(ranked.len(), 2);
        assert!((ranked[0].confidence - 0.88).abs() < 1e-6);
        assert!(ranked[1].confidence <= DIFFERENT_VERSION_CAP);
    }

    #[test]
    fn fusion_prefers_non_compilation_album_and_discogs_real_label() {
        let known = KnownTags {
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            ..KnownTags::default()
        };
        let deezer = Candidate {
            provider: "deezer".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            album: Some("Night Drive EP".to_owned()),
            label: Some("Noisy Streaming Imprint".to_owned()),
            duration_ms: Some(360_000),
            release_kind: ReleaseKind::SingleOrEp,
            ..Candidate::default()
        };
        let discogs = Candidate {
            provider: "discogs".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            album: Some("Club Anthems 2024".to_owned()),
            label: Some("Defected".to_owned()),
            catno: Some("DFTD100".to_owned()),
            release_kind: ReleaseKind::Compilation,
            ..Candidate::default()
        };
        let ranked = fuse_and_rank(&known, Some(360_000), vec![deezer, discogs]);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].candidate.album.as_deref(), Some("Night Drive EP"));
        assert_eq!(ranked[0].candidate.label.as_deref(), Some("Defected"));
    }

    #[test]
    fn expands_candidates_when_alternative_adds_legal_artwork() {
        let top = RankedCandidate {
            candidate: Candidate {
                provider: "deezer".to_owned(),
                artist: Some("Some Artist".to_owned()),
                title: Some("Night Drive".to_owned()),
                catno: Some("CAT01".to_owned()),
                ..Candidate::default()
            },
            confidence: 0.98,
            audio_matched: false,
            duration_delta_ms: Some(200),
            providers: vec!["deezer".to_owned()],
            evidence: MatchEvidence::default(),
        };
        let alt = RankedCandidate {
            candidate: Candidate {
                provider: "discogs".to_owned(),
                artist: Some("Some Artist".to_owned()),
                title: Some("Night Drive".to_owned()),
                artwork_url: Some("https://i.discogs.com/cover.jpg".to_owned()),
                artwork_provider: Some("discogs".to_owned()),
                ..Candidate::default()
            },
            confidence: 0.84,
            audio_matched: false,
            duration_delta_ms: None,
            providers: vec!["discogs".to_owned()],
            evidence: MatchEvidence::default(),
        };
        assert!(should_expand_candidates(&[top, alt]));
    }

    #[test]
    fn retain_viable_candidates_filters_low_confidence_and_sibling_release_tracks() {
        let known = KnownTags {
            artist: Some("Artist A".to_owned()),
            title: Some("Main Track".to_owned()),
            ..KnownTags::default()
        };
        let ranked = vec![
            RankedCandidate {
                candidate: Candidate {
                    provider: "discogs".to_owned(),
                    artist: Some("Artist A".to_owned()),
                    title: Some("Main Track".to_owned()),
                    url: Some("https://www.discogs.com/release/100".to_owned()),
                    ..Candidate::default()
                },
                confidence: 0.96,
                audio_matched: false,
                duration_delta_ms: Some(300),
                providers: vec!["discogs".to_owned()],
                evidence: MatchEvidence::default(),
            },
            RankedCandidate {
                candidate: Candidate {
                    provider: "discogs".to_owned(),
                    artist: Some("Artist B".to_owned()),
                    title: Some("Unrelated Flip".to_owned()),
                    url: Some("https://www.discogs.com/release/100".to_owned()),
                    ..Candidate::default()
                },
                confidence: 0.68,
                audio_matched: false,
                duration_delta_ms: None,
                providers: vec!["discogs".to_owned()],
                evidence: MatchEvidence::default(),
            },
            RankedCandidate {
                candidate: Candidate {
                    provider: "discogs".to_owned(),
                    artist: Some("Artist A".to_owned()),
                    title: Some("Main Track".to_owned()),
                    mix: Some("Dub".to_owned()),
                    url: Some("https://www.discogs.com/release/100".to_owned()),
                    ..Candidate::default()
                },
                confidence: 0.72,
                audio_matched: false,
                duration_delta_ms: None,
                providers: vec!["discogs".to_owned()],
                evidence: MatchEvidence::default(),
            },
            RankedCandidate {
                candidate: Candidate {
                    provider: "musicbrainz".to_owned(),
                    artist: Some("Artist A".to_owned()),
                    title: Some("Main Track".to_owned()),
                    url: Some("https://musicbrainz.org/release/rel-1".to_owned()),
                    ..Candidate::default()
                },
                confidence: 0.58,
                audio_matched: false,
                duration_delta_ms: None,
                providers: vec!["musicbrainz".to_owned()],
                evidence: MatchEvidence::default(),
            },
        ];
        let kept = retain_viable_candidates(ranked);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].candidate.title.as_deref(), Some("Main Track"));
        assert_eq!(kept[1].candidate.mix.as_deref(), Some("Dub"));

        let low = rank(
            &known,
            Some(300_000),
            vec![Candidate {
                provider: "discogs".to_owned(),
                artist: Some("Other Act".to_owned()),
                title: Some("Different Tune".to_owned()),
                ..Candidate::default()
            }],
        );
        assert!(retain_viable_candidates(low).is_empty());

        let weak_tags_only = RankedCandidate {
            candidate: Candidate {
                provider: "itunes".to_owned(),
                artist: Some("Artist A".to_owned()),
                title: Some("Main Track".to_owned()),
                ..Candidate::default()
            },
            confidence: 0.57,
            audio_matched: false,
            duration_delta_ms: None,
            providers: vec!["itunes".to_owned()],
            evidence: MatchEvidence {
                basis: MatchBasis::TagsOnly,
                ..MatchEvidence::default()
            },
        };
        assert!(!is_viable_candidate(&weak_tags_only));
        assert!(retain_viable_candidates(vec![weak_tags_only]).is_empty());

        let strong_tags_only = RankedCandidate {
            candidate: Candidate {
                provider: "discogs".to_owned(),
                artist: Some("Artist A".to_owned()),
                title: Some("Main Track".to_owned()),
                ..Candidate::default()
            },
            confidence: 0.74,
            audio_matched: false,
            duration_delta_ms: None,
            providers: vec!["discogs".to_owned()],
            evidence: MatchEvidence {
                basis: MatchBasis::TagsOnly,
                ..MatchEvidence::default()
            },
        };
        assert!(is_viable_candidate(&strong_tags_only));
        assert!(should_expand_candidates(&[strong_tags_only]));
    }
}
