//! How a track identification and its step reports are stored as JSON.

use crate::providers::PROVIDER_IDS;
use resonance_core::identify::{
    AudioMatchSource, KnownTags, MatchBasis, MatchEvidence, RankedCandidate, ReleaseKind, Step,
    StepOutcome,
};
use resonance_core::library::TagField;
use resonance_core::providers::ProviderKey;
use resonance_db::StoredIdentification;
use serde_json::{Value, json};

use crate::pipeline::{Identification, StepReport};

pub(crate) fn stored(identification: &Identification, online: bool) -> StoredIdentification {
    let best_json = identification
        .best
        .as_ref()
        .map(|best| ranked_candidate_json(best).to_string());
    let candidates_json = Value::Array(
        identification
            .candidates
            .iter()
            .map(ranked_candidate_json)
            .collect(),
    )
    .to_string();

    StoredIdentification {
        track_id: identification.track_id,
        online,
        confidence: identification.best.as_ref().map(|best| best.confidence),
        audio_matched: identification
            .best
            .as_ref()
            .is_some_and(|best| best.audio_matched),
        known_json: known_json(&identification.known).to_string(),
        best_json,
        steps_json: Value::Array(
            identification
                .steps
                .iter()
                .map(|report| {
                    let mut value = outcome_json(&report.outcome);
                    value["step"] = json!(report.step.name());
                    value["provider"] = json!(report.provider);
                    value
                })
                .collect(),
        )
        .to_string(),
        candidates_json,
        selected_candidate: 0,
        updated_ns: 0,
    }
}

/// Parses stored `steps_json` back into `Vec<StepReport>`.
pub fn parse_step_reports(steps_json: &str) -> Vec<StepReport> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(steps_json) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|v| {
            let step = Step::from_name(v.get("step")?.as_str()?)?;
            let provider = v
                .get("provider")
                .and_then(Value::as_str)
                .and_then(static_provider_id);
            let outcome = match v.get("outcome")?.as_str()? {
                "read" => StepOutcome::Read {
                    fields: v.get("fields").and_then(Value::as_u64).unwrap_or(0) as usize,
                },
                "ran" => StepOutcome::Ran {
                    candidates: v.get("candidates").and_then(Value::as_u64).unwrap_or(0) as usize,
                    best: v.get("best").and_then(Value::as_f64),
                },
                "skipped_missing_key" => StepOutcome::SkippedMissingKey(provider_key_from_account(
                    v.get("key").and_then(Value::as_str)?,
                )?),
                "skipped_no_input" => StepOutcome::SkippedNoInput,
                "skipped_confident" => StepOutcome::SkippedConfident,
                "failed" => StepOutcome::Failed(
                    v.get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error")
                        .to_owned(),
                ),
                "needs_url" => StepOutcome::NeedsUrl,
                _ => return None,
            };
            Some(StepReport {
                step,
                provider,
                outcome,
            })
        })
        .collect()
}

/// Parses stored `candidates_json` back into `Vec<RankedCandidate>`.
pub fn parse_ranked_candidates(json_str: &str) -> Vec<RankedCandidate> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|v| ranked_from_json(&v))
        .collect()
}

/// Parses `MatchEvidence` from either a `best_json` or `candidates_json[i]` JSON object.
pub fn parse_match_evidence(v: &Value) -> Option<MatchEvidence> {
    let obj = v.get("evidence")?;
    let basis = obj
        .get("basis")
        .and_then(Value::as_str)
        .map(basis_from_str)
        .unwrap_or(MatchBasis::TagsOnly);
    let providers = obj
        .get("providers")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let primary_provider = obj
        .get("primary_provider")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Some(MatchEvidence {
        basis,
        providers,
        primary_provider,
        duration_delta_ms: obj.get("duration_delta_ms").and_then(Value::as_u64),
        preview_similarity: obj
            .get("preview_similarity")
            .and_then(Value::as_f64)
            .map(|f| f as f32),
        catno_matched: obj
            .get("catno_matched")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        isrc_matched: obj
            .get("isrc_matched")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        url: obj.get("url").and_then(Value::as_str).map(str::to_owned),
    })
}

pub(crate) fn ranked_candidate_json(ranked: &RankedCandidate) -> Value {
    let c = &ranked.candidate;
    let field_providers: serde_json::Map<String, Value> = c
        .field_providers
        .iter()
        .map(|(f, p)| (f.name().to_owned(), json!(p)))
        .collect();
    json!({
        "provider": c.provider,
        "providers": ranked.providers,
        "confidence": ranked.confidence,
        "audio_matched": ranked.audio_matched,
        "artist": c.artist,
        "title": c.title,
        "mix": c.mix,
        "album": c.album,
        "label": c.label,
        "catno": c.catno,
        "year": c.year,
        "genre": c.genre,
        "subgenre": c.subgenre,
        "track_number": c.track_number,
        "isrc": c.isrc,
        "duration_ms": c.duration_ms,
        "duration_delta_ms": ranked.duration_delta_ms,
        "recording_id": c.recording_id,
        "release_id": c.release_id,
        "url": c.url,
        "artwork_url": c.artwork_url,
        "artwork_provider": c.artwork_provider,
        "release_kind": release_kind_str(c.release_kind),
        "fingerprint_score": c.fingerprint_score,
        "preview_similarity": c.preview_similarity,
        "audio_source": c.audio_source.map(audio_source_str),
        "user_supplied": c.user_supplied,
        "from_embedded_url": c.from_embedded_url,
        "artist_is_seller": c.artist_is_seller,
        "field_providers": field_providers,
        "evidence": {
            "basis": basis_str(ranked.evidence.basis),
            "providers": ranked.evidence.providers,
            "primary_provider": ranked.evidence.primary_provider,
            "duration_delta_ms": ranked.evidence.duration_delta_ms,
            "preview_similarity": ranked.evidence.preview_similarity,
            "catno_matched": ranked.evidence.catno_matched,
            "isrc_matched": ranked.evidence.isrc_matched,
            "url": ranked.evidence.url,
        },
    })
}

fn known_json(known: &KnownTags) -> Value {
    json!({
        "artist": known.artist,
        "title": known.title,
        "mix": known.mix,
        "album": known.album,
        "label": known.label,
        "catno": known.catno,
        "year": known.year,
        "track_number": known.track_number,
        "remixer": known.remixer,
        "isrc": known.isrc,
    })
}

fn ranked_from_json(v: &Value) -> Option<RankedCandidate> {
    let provider = v.get("provider")?.as_str()?.to_owned();
    let confidence = v.get("confidence")?.as_f64()?;
    let audio_matched = v.get("audio_matched")?.as_bool()?;
    let str_field = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let field_providers = v
        .get("field_providers")
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, val)| Some((TagField::from_name(k)?, val.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let candidate = resonance_core::identify::Candidate {
        provider,
        artist: str_field("artist"),
        title: str_field("title"),
        mix: str_field("mix"),
        album: str_field("album"),
        label: str_field("label"),
        catno: str_field("catno"),
        year: v
            .get("year")
            .and_then(Value::as_u64)
            .and_then(|y| u16::try_from(y).ok()),
        duration_ms: v.get("duration_ms").and_then(Value::as_u64),
        genre: str_field("genre"),
        subgenre: str_field("subgenre"),
        track_number: str_field("track_number"),
        isrc: str_field("isrc"),
        recording_id: str_field("recording_id"),
        release_id: str_field("release_id"),
        url: str_field("url"),
        artwork_url: str_field("artwork_url"),
        artwork_provider: str_field("artwork_provider"),
        preview_url: None,
        release_kind: v
            .get("release_kind")
            .and_then(Value::as_str)
            .map(release_kind_from_str)
            .unwrap_or(ReleaseKind::Unknown),
        fingerprint_score: v.get("fingerprint_score").and_then(Value::as_f64),
        preview_similarity: v
            .get("preview_similarity")
            .and_then(Value::as_f64)
            .map(|f| f as f32),
        audio_source: v
            .get("audio_source")
            .and_then(Value::as_str)
            .and_then(audio_source_from_str),
        user_supplied: v
            .get("user_supplied")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        from_embedded_url: v
            .get("from_embedded_url")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        artist_is_seller: v
            .get("artist_is_seller")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        field_providers,
    };
    let providers = v
        .get("providers")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_else(|| vec![candidate.provider.clone()]);
    let evidence = parse_match_evidence(v).unwrap_or_default();
    Some(RankedCandidate {
        candidate,
        confidence,
        audio_matched,
        duration_delta_ms: v.get("duration_delta_ms").and_then(Value::as_u64),
        providers,
        evidence,
    })
}

fn static_provider_id(name: &str) -> Option<&'static str> {
    PROVIDER_IDS.into_iter().find(|id| *id == name)
}

fn provider_key_from_account(account: &str) -> Option<ProviderKey> {
    ProviderKey::ALL
        .into_iter()
        .find(|key| key.account() == account)
}

fn basis_str(basis: MatchBasis) -> &'static str {
    match basis {
        MatchBasis::AudioFingerprint => "audio_fingerprint",
        MatchBasis::AudioPreview => "audio_preview",
        MatchBasis::AudioRecogniser => "audio_recogniser",
        MatchBasis::UserUrl => "user_url",
        MatchBasis::EmbeddedUrl => "embedded_url",
        MatchBasis::CatalogAndDuration => "catalog_and_duration",
        MatchBasis::TagsAndDuration => "tags_and_duration",
        MatchBasis::TagsOnly => "tags_only",
        MatchBasis::LocalTags => "local_tags",
    }
}

fn basis_from_str(s: &str) -> MatchBasis {
    match s {
        "audio_fingerprint" => MatchBasis::AudioFingerprint,
        "audio_preview" => MatchBasis::AudioPreview,
        "audio_recogniser" => MatchBasis::AudioRecogniser,
        "user_url" => MatchBasis::UserUrl,
        "embedded_url" => MatchBasis::EmbeddedUrl,
        "catalog_and_duration" => MatchBasis::CatalogAndDuration,
        "tags_and_duration" => MatchBasis::TagsAndDuration,
        "local_tags" => MatchBasis::LocalTags,
        _ => MatchBasis::TagsOnly,
    }
}

fn release_kind_str(kind: ReleaseKind) -> &'static str {
    match kind {
        ReleaseKind::SingleOrEp => "single_or_ep",
        ReleaseKind::Album => "album",
        ReleaseKind::Compilation => "compilation",
        ReleaseKind::Unknown => "unknown",
    }
}

fn release_kind_from_str(s: &str) -> ReleaseKind {
    match s {
        "single_or_ep" => ReleaseKind::SingleOrEp,
        "album" => ReleaseKind::Album,
        "compilation" => ReleaseKind::Compilation,
        _ => ReleaseKind::Unknown,
    }
}

fn audio_source_str(src: AudioMatchSource) -> &'static str {
    match src {
        AudioMatchSource::Fingerprint => "fingerprint",
        AudioMatchSource::PreviewMatch => "preview_match",
        AudioMatchSource::Recogniser => "recogniser",
    }
}

fn audio_source_from_str(s: &str) -> Option<AudioMatchSource> {
    match s {
        "fingerprint" => Some(AudioMatchSource::Fingerprint),
        "preview_match" => Some(AudioMatchSource::PreviewMatch),
        "recogniser" => Some(AudioMatchSource::Recogniser),
        _ => None,
    }
}

fn outcome_json(outcome: &StepOutcome) -> Value {
    match outcome {
        StepOutcome::Read { fields } => json!({ "outcome": "read", "fields": fields }),
        StepOutcome::Ran { candidates, best } => {
            json!({ "outcome": "ran", "candidates": candidates, "best": best })
        }
        StepOutcome::SkippedMissingKey(key) => {
            json!({ "outcome": "skipped_missing_key", "key": key.account() })
        }
        StepOutcome::SkippedNoInput => json!({ "outcome": "skipped_no_input" }),
        StepOutcome::SkippedConfident => json!({ "outcome": "skipped_confident" }),
        StepOutcome::Failed(reason) => json!({ "outcome": "failed", "reason": reason }),
        StepOutcome::NeedsUrl => json!({ "outcome": "needs_url" }),
    }
}
