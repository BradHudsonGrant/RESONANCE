//! Checking a match by ear: a store's 30-second preview is fingerprinted in memory and slid
//! along the file's own fingerprint. When that still leaves a track uncertain, the paid AudD
//! recogniser is asked, but only if the user has entered a token.

use std::sync::PoisonError;
use std::sync::atomic::{AtomicBool, Ordering};

use resonance_core::identify::{
    AudioMatchSource, FINGERPRINT_PREFIX_MS, LONGER_EDIT_MIX, Step, StepOutcome, is_other_edit,
};
use resonance_core::identity::match_preview_fingerprint;

use crate::pipeline::{Pipeline, TrackState, ran, report};
use crate::provider::{Query, QueryKind};

/// Maximum 30-second preview downloads per track during step 5.
const MAX_PREVIEW_CHECKS: usize = 2;

impl Pipeline {
    pub(crate) fn verify_audio_previews(
        &self,
        state: &mut TrackState,
        keys: &crate::pipeline::Keys,
        abort: &AtomicBool,
    ) -> Option<usize> {
        let fingerprinter = self.fingerprinter.as_ref()?;
        let audio_identity = {
            let db = self.database.lock().unwrap_or_else(PoisonError::into_inner);
            db.asset_audio_identity(state.identification.track_id)
                .ok()
                .flatten()
        };
        let audio_identity = audio_identity?;
        let full_path =
            fingerprinter.resolve_asset_path(&audio_identity.source_folder, &audio_identity.path);

        let mut scored_indices: Vec<(usize, f64)> = state
            .collected
            .iter()
            .enumerate()
            .filter(|(_, c)| c.preview_url.is_some())
            .map(|(i, c)| {
                let conf = resonance_core::identify::score_confidence(
                    &state.identification.known,
                    state.duration_ms,
                    c,
                );
                (i, conf)
            })
            .collect();
        scored_indices.sort_by(|a, b| b.1.total_cmp(&a.1));

        let mut full_fp_cache: Option<Vec<u32>> = None;
        let mut confirmed = false;
        let mut compared = false;
        let mut follow_up_hint: Option<(String, String)> = None;

        for (idx, _) in scored_indices.into_iter().take(MAX_PREVIEW_CHECKS) {
            if abort.load(Ordering::Acquire) {
                break;
            }
            let (provider_id, preview_url) = {
                let c = &state.collected[idx];
                (c.provider.clone(), c.preview_url.clone())
            };
            let Some(url) = preview_url else { continue };
            let Some(registered) = self.find_registered(&provider_id) else {
                continue;
            };
            let Ok(Some(bytes)) =
                self.fetcher
                    .fetch_preview(registered.provider.id(), &registered.limiter, &url)
            else {
                continue;
            };
            let ext_hint = url
                .split(['?', '#'])
                .next()
                .and_then(|path| path.rsplit('.').next())
                .filter(|clean| matches!(*clean, "mp3" | "m4a" | "aac"));
            let Ok(preview_fp) = fingerprinter.fingerprint_bytes(&bytes, ext_hint) else {
                continue;
            };

            let mut matched = audio_identity
                .fingerprint_raw
                .as_deref()
                .and_then(|raw| {
                    compared = true;
                    match_preview_fingerprint(raw, &preview_fp)
                })
                .filter(|m| m.confirmed);

            if matched.is_none()
                && state
                    .duration_ms
                    .is_none_or(|ms| ms > FINGERPRINT_PREFIX_MS)
                && full_path.is_file()
            {
                if full_fp_cache.is_none()
                    && let Ok(full_fp) = fingerprinter.fingerprint_file_full(&full_path)
                {
                    full_fp_cache = Some(full_fp);
                }
                if let Some(full_fp) = &full_fp_cache {
                    compared = true;
                    matched =
                        match_preview_fingerprint(full_fp, &preview_fp).filter(|m| m.confirmed);
                }
            }

            if let Some(m) = matched {
                let cand = &mut state.collected[idx];
                if is_other_edit(state.duration_ms, cand.duration_ms) {
                    if let (Some(a), Some(t)) = (&cand.artist, &cand.title) {
                        follow_up_hint = Some((a.clone(), t.clone()));
                    }
                } else {
                    cand.preview_similarity = Some(m.similarity);
                    cand.audio_source = Some(AudioMatchSource::PreviewMatch);
                    confirmed = true;
                    break;
                }
            }
        }

        // The preview is the same recording in another edit, so it proves nothing about the file.
        // Search for the longer edit by the names the preview confirmed instead.
        if !confirmed && let Some((confirmed_artist, confirmed_title)) = follow_up_hint {
            self.extended_followup_search(state, &confirmed_artist, &confirmed_title, keys);
        }

        compared.then_some(usize::from(confirmed))
    }

    pub(crate) fn extended_followup_search(
        &self,
        state: &mut TrackState,
        artist: &str,
        title: &str,
        keys: &crate::pipeline::Keys,
    ) {
        let mut query_known = state.identification.known.clone();
        query_known.artist = Some(artist.to_owned());
        query_known.title = Some(title.to_owned());
        if query_known.mix.is_none() {
            query_known.mix = Some(LONGER_EDIT_MIX.to_owned());
        }
        for id in ["deezer", "discogs", "musicbrainz"] {
            let Some(registered) = self.find_registered(id) else {
                continue;
            };
            if self.blocked(registered, QueryKind::Text, keys).is_some() {
                continue;
            }
            let lookup = self.lookup(registered, keys);
            if let Ok(found) = registered
                .provider
                .lookup(&Query::Text(&query_known), &lookup)
            {
                state.collected.extend(found);
            }
        }
    }

    pub(crate) fn audd_fallback_step(&self, state: &mut TrackState, keys: &crate::pipeline::Keys) {
        if state.done {
            return;
        }
        let Some(audd) = self.find_registered("audd") else {
            return;
        };
        if let Some(outcome) = self.blocked(audd, QueryKind::AudioExcerpt, keys) {
            report(state, Step::AudioConfirmation, Some("audd"), outcome);
            return;
        }
        let Some(fingerprinter) = &self.fingerprinter else {
            report(
                state,
                Step::AudioConfirmation,
                Some("audd"),
                StepOutcome::SkippedNoInput,
            );
            return;
        };
        let audio_identity = {
            let db = self.database.lock().unwrap_or_else(PoisonError::into_inner);
            db.asset_audio_identity(state.identification.track_id)
                .ok()
                .flatten()
        };
        let Some(audio_identity) = audio_identity else {
            report(
                state,
                Step::AudioConfirmation,
                Some("audd"),
                StepOutcome::SkippedNoInput,
            );
            return;
        };
        let full_path =
            fingerprinter.resolve_asset_path(&audio_identity.source_folder, &audio_identity.path);

        let Ok(wav_bytes) = fingerprinter.extract_wav_excerpt(&full_path, 0.40, 12) else {
            report(
                state,
                Step::AudioConfirmation,
                Some("audd"),
                StepOutcome::SkippedNoInput,
            );
            return;
        };
        let hash_hex = audio_identity
            .audio_hash
            .map(|h| h.to_hex())
            .unwrap_or_else(|| blake3::hash(&wav_bytes).to_hex().to_string());
        let lookup = self.lookup(audd, keys);

        match audd.provider.lookup(
            &Query::AudioExcerpt {
                wav_bytes: &wav_bytes,
                audio_hash_hex: &hash_hex,
            },
            &lookup,
        ) {
            Ok(found) => {
                let count = found.len();
                let mut followup_pair: Option<(String, String)> = None;
                let mut isrcs: Vec<String> = Vec::new();
                for cand in found {
                    if let Some(isrc) = &cand.isrc {
                        isrcs.push(isrc.clone());
                    }
                    if is_other_edit(state.duration_ms, cand.duration_ms) {
                        if let (Some(a), Some(t)) = (&cand.artist, &cand.title) {
                            followup_pair = Some((a.clone(), t.clone()));
                        }
                    } else {
                        state.collected.push(cand);
                    }
                }
                for isrc in isrcs {
                    for registered in self.answering(QueryKind::Isrc) {
                        if self.blocked(registered, QueryKind::Isrc, keys).is_some() {
                            continue;
                        }
                        let lookup = self.lookup(registered, keys);
                        if let Ok(hits) = registered.provider.lookup(&Query::Isrc(&isrc), &lookup) {
                            state.collected.extend(hits);
                        }
                    }
                }
                if let Some((a, t)) = followup_pair {
                    self.extended_followup_search(state, &a, &t, keys);
                }
                let best = self.settle(state);
                report(
                    state,
                    Step::AudioConfirmation,
                    Some("audd"),
                    ran(count, best),
                );
            }
            Err(error) => report(
                state,
                Step::AudioConfirmation,
                Some("audd"),
                StepOutcome::Failed(error.to_string()),
            ),
        }
    }
}
