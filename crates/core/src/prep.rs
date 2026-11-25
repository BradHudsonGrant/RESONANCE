//! Batch tagging rules, release grouping and local proposals for Prep.
//!
//! Nothing here touches the database, the network or the file system.

use crate::identify::{normalise_tags, parse_path};
use crate::library::TagField;
use crate::search::SortDirection;
use crate::staging::{
    Proposal, ProposalStatus, StoredProposal, changing_proposals, normalise_value, title_with_mix,
};

/// The release-level fields copied across an album, EP or folder by "Apply to release".
pub const RELEASE_FIELDS: [TagField; 4] = [
    TagField::Album,
    TagField::Label,
    TagField::CatNo,
    TagField::Year,
];

/// Text fields covered when Fix Case runs with no explicit field list.
pub const CASE_FIX_DEFAULT_FIELDS: [TagField; 6] = [
    TagField::Title,
    TagField::Artist,
    TagField::Mix,
    TagField::Album,
    TagField::Label,
    TagField::Genre,
];

/// Text fields covered when Find and Replace runs across all text tags.
pub const BATCH_TEXT_FIELDS: [TagField; 11] = [
    TagField::Title,
    TagField::Artist,
    TagField::Mix,
    TagField::Album,
    TagField::AlbumArtist,
    TagField::Genre,
    TagField::Subgenre,
    TagField::Label,
    TagField::CatNo,
    TagField::Comment,
    TagField::Remixer,
];

/// How a batch case-fix rewrites a tag's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCase {
    Title,
    Sentence,
    Upper,
}

/// Acronyms and Roman numerals that stay uppercase in Title and Sentence case.
const DJ_ACRONYMS: [&str; 25] = [
    "UK", "DJ", "EP", "VIP", "LP", "MC", "II", "III", "IV", "VI", "VII", "VIII", "IX", "XI", "XII",
    "ID", "BPM", "UKG", "DNB", "NYC", "US", "USA", "PA", "FX", "EQ",
];

/// Plural acronyms where the acronym stays uppercase and the trailing `s` is lowercase.
const PLURAL_ACRONYMS: [(&str, &str); 6] = [
    ("DJS", "DJs"),
    ("MCS", "MCs"),
    ("EPS", "EPs"),
    ("LPS", "LPs"),
    ("VIPS", "VIPs"),
    ("IDS", "IDs"),
];

/// Tokens that stay lowercase inside Title and Sentence case unless they start the string.
const ALWAYS_LOWER_TOKENS: [&str; 6] = ["feat", "feat.", "ft", "ft.", "vs", "vs."];

/// Short connecting words kept lowercase inside Title Case unless first, last or after a break.
const TITLE_MINOR_WORDS: [&str; 23] = [
    "a", "an", "and", "as", "at", "but", "by", "for", "from", "in", "into", "nor", "of", "on",
    "onto", "or", "over", "per", "the", "to", "up", "via", "with",
];

/// Rewrites `value` in `case`, keeping DJ and format acronyms (`UK`, `DJ`, `EP`, `VIP`, `LP`,
/// `MC`, `II` and friends) uppercase and collapsing stray whitespace.
pub fn fix_case(value: &str, case: TextCase) -> String {
    let words: Vec<&str> = value.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    if case == TextCase::Upper {
        return words.join(" ").to_uppercase();
    }

    let count = words.len();
    let mut after_break = true;
    let mut out = Vec::with_capacity(count);

    for (index, raw) in words.into_iter().enumerate() {
        let is_first = index == 0;
        let is_last = index + 1 == count;
        let ends_with_break = raw.ends_with(['-', ':', '/', '–', '—']);
        let is_break_token = matches!(raw, "-" | ":" | "/" | "–" | "—");
        if is_break_token {
            out.push(raw.to_owned());
            after_break = true;
            continue;
        }

        let formatted = format_token(raw, case, is_first, is_last, after_break);
        after_break = ends_with_break;
        out.push(formatted);
    }

    out.join(" ")
}

fn format_token(
    raw: &str,
    case: TextCase,
    is_first: bool,
    is_last: bool,
    after_break: bool,
) -> String {
    let (prefix, rest) = split_leading_punct(raw);
    let (core, suffix) = split_trailing_punct(rest);
    if core.is_empty() {
        return raw.to_owned();
    }

    let opened_bracket = prefix.contains(['(', '[', '{', '"']);
    let lower_with_dot = format!("{}{}", core.to_lowercase(), suffix.to_lowercase());
    if !is_first && ALWAYS_LOWER_TOKENS.contains(&lower_with_dot.as_str()) {
        return format!("{prefix}{lower_with_dot}");
    }
    let core_lower = core.to_lowercase();
    if !is_first && ALWAYS_LOWER_TOKENS.contains(&core_lower.as_str()) {
        return format!("{prefix}{core_lower}{suffix}");
    }

    if core.contains('-') || core.contains('/') {
        let sep = if core.contains('/') { '/' } else { '-' };
        let parts: Vec<&str> = core.split(sep).collect();
        let joined = parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                format_word(
                    part,
                    case,
                    is_first && i == 0,
                    is_last && i + 1 == parts.len(),
                    (after_break || opened_bracket) && i == 0,
                )
            })
            .collect::<Vec<_>>()
            .join(&sep.to_string());
        return format!("{prefix}{joined}{suffix}");
    }

    let word = format_word(core, case, is_first, is_last, after_break || opened_bracket);
    format!("{prefix}{word}{suffix}")
}

fn format_word(
    core: &str,
    case: TextCase,
    is_first: bool,
    is_last: bool,
    after_break: bool,
) -> String {
    let upper = core.to_ascii_uppercase();
    if DJ_ACRONYMS.contains(&upper.as_str()) {
        return upper;
    }
    if let Some((_, plural)) = PLURAL_ACRONYMS.iter().find(|(key, _)| *key == upper) {
        return (*plural).to_owned();
    }
    let lower = core.to_lowercase();
    match case {
        TextCase::Upper => core.to_uppercase(),
        TextCase::Sentence => {
            if is_first {
                capitalise(&lower)
            } else {
                lower
            }
        }
        TextCase::Title => {
            if !is_first && !is_last && !after_break && TITLE_MINOR_WORDS.contains(&lower.as_str())
            {
                lower
            } else {
                capitalise(&lower)
            }
        }
    }
}

fn capitalise(lower: &str) -> String {
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn split_leading_punct(token: &str) -> (&str, &str) {
    let idx = token
        .char_indices()
        .find(|(_, c)| c.is_alphanumeric())
        .map_or(token.len(), |(i, _)| i);
    token.split_at(idx)
}

fn split_trailing_punct(token: &str) -> (&str, &str) {
    let end = token
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphanumeric())
        .map_or(0, |(i, c)| i + c.len_utf8());
    token.split_at(end)
}

/// Replaces occurrences of `find` with `replace` inside `value`, collapsing double spaces and
/// empty brackets left behind. Returns `None` when `find` is empty or the value is unchanged.
pub fn find_and_replace(
    value: &str,
    find: &str,
    replace: &str,
    case_sensitive: bool,
) -> Option<String> {
    if find.is_empty() || value.is_empty() {
        return None;
    }

    let replaced = if case_sensitive {
        if !value.contains(find) {
            return None;
        }
        value.replace(find, replace)
    } else {
        replace_case_insensitive(value, find, replace)?
    };

    let cleaned = collapse_spaces_and_empty_brackets(&replaced);
    (cleaned != value.trim()).then_some(cleaned)
}

fn replace_case_insensitive(value: &str, find: &str, replace: &str) -> Option<String> {
    let haystack_lower = value.to_lowercase();
    let needle_lower = find.to_lowercase();
    if needle_lower.is_empty() || !haystack_lower.contains(&needle_lower) {
        return None;
    }
    // When lowercasing preserves byte offsets (ASCII and most Latin characters), slice the
    // original string directly so untouched characters keep their exact casing.
    if haystack_lower.len() == value.len() && needle_lower.len() == find.len() {
        let mut out = String::with_capacity(value.len());
        let mut cursor = 0;
        let mut matched = false;
        while let Some(pos) = haystack_lower[cursor..].find(&needle_lower) {
            let start = cursor + pos;
            let end = start + needle_lower.len();
            if !value.is_char_boundary(start) || !value.is_char_boundary(end) {
                break;
            }
            out.push_str(&value[cursor..start]);
            out.push_str(replace);
            cursor = end;
            matched = true;
        }
        if matched {
            out.push_str(&value[cursor..]);
            return Some(out);
        }
    }
    None
}

fn collapse_spaces_and_empty_brackets(text: &str) -> String {
    let without_empty = text
        .replace("()", "")
        .replace("[]", "")
        .replace("( )", "")
        .replace("[ ]", "");
    without_empty
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where a track stands in Prep. The order is the order a user works through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrepStage {
    /// An online match's suggestions are waiting for a decision.
    Review,
    /// No online search has run for the track yet. Tidy-ups read from its own tags or name may
    /// be waiting, but nothing has been matched.
    NotSearched,
    /// An online search ran and found nothing usable. Tidy-ups from the file may be waiting.
    NoMatch,
    /// Matched online and every suggestion has been saved or skipped.
    Done,
}

impl PrepStage {
    /// Only an online match puts a track up for review. Tidy-ups read from the file alone do
    /// not: a track with nothing but those is still waiting to be searched, and the list says so.
    pub fn of(has_pending: bool, searched_online: bool, matched_online: bool) -> Self {
        if matched_online {
            if has_pending {
                Self::Review
            } else {
                Self::Done
            }
        } else if searched_online {
            Self::NoMatch
        } else {
            Self::NotSearched
        }
    }
}

/// The tags a DJ notices are missing, in the order a row names them. BPM and key come from
/// analysis, so they are left out.
pub const NOTICED_TAGS: [TagField; 5] = [
    TagField::Artist,
    TagField::Title,
    TagField::Genre,
    TagField::Label,
    TagField::Year,
];

/// Groups tracks by stage, keeping the view's own order inside each stage. Ascending puts the
/// tracks waiting for a decision first.
pub fn order_by_stage(
    ids: &mut [i64],
    stage_of: impl Fn(i64) -> PrepStage,
    direction: SortDirection,
) {
    match direction {
        SortDirection::Asc => ids.sort_by_key(|&id| stage_of(id)),
        SortDirection::Desc => ids.sort_by_key(|&id| std::cmp::Reverse(stage_of(id))),
    }
}

/// The parent folder of a relative asset path (`"Label - EP/01 Track.wav"` -> `"Label - EP"`).
pub fn parent_folder(path: &str) -> &str {
    path.rfind(['/', '\\']).map_or("", |index| &path[..index])
}

/// How Prep groups tracks that belong to the same album, EP or release folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseGroup {
    pub key: String,
    pub title: String,
    pub subtitle: Option<String>,
}

/// Builds a release grouping key and display title from a track's effective or proposed tags and
/// its relative path. Tracks with no album, catalogue number or release folder return `None`.
pub fn release_group(
    path: &str,
    album: Option<&str>,
    label: Option<&str>,
    catno: Option<&str>,
    year: Option<&str>,
) -> Option<ReleaseGroup> {
    fn clean(opt: Option<&str>) -> Option<&str> {
        opt.map(str::trim).filter(|s| !s.is_empty())
    }
    let album = clean(album);
    let label = clean(label);
    let catno = clean(catno);
    let year = clean(year);
    let folder = parent_folder(path).trim();
    let leaf_folder = folder
        .rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let key = match (catno, album, leaf_folder) {
        (Some(cat), Some(alb), _) => {
            format!(
                "release:{}:{}",
                cat.to_ascii_lowercase(),
                alb.to_lowercase()
            )
        }
        (Some(cat), None, _) => format!("catno:{}", cat.to_ascii_lowercase()),
        (None, Some(alb), _) => format!(
            "album:{}:{}",
            alb.to_lowercase(),
            label.unwrap_or_default().to_lowercase()
        ),
        (None, None, Some(dir)) => format!("folder:{}", dir.to_lowercase()),
        (None, None, None) => return None,
    };

    let title = album
        .or(leaf_folder)
        .or(catno)
        .unwrap_or("Release")
        .to_owned();
    let parts: Vec<&str> = [label, catno, year].into_iter().flatten().collect();
    let subtitle = (!parts.is_empty()).then(|| parts.join(" · "));

    Some(ReleaseGroup {
        key,
        title,
        subtitle,
    })
}

/// Proposals from steps 1 and 2 (normalised existing tags and filename/folder parsing) that
/// would change or fill what `file_tags` holds.
pub fn local_proposals(file_tags: &[(TagField, String)], path: &str) -> Vec<Proposal> {
    let from_tags = normalise_tags(file_tags);
    let from_path = parse_path(path);

    let has_tag = |wanted: TagField| {
        file_tags
            .iter()
            .any(|(field, value)| *field == wanted && !value.trim().is_empty())
    };
    let path_has_structure = from_path.artist.is_some()
        || from_path.mix.is_some()
        || from_path.album.is_some()
        || from_path.label.is_some()
        || from_path.catno.is_some()
        || from_path.year.is_some()
        || from_path.track_number.is_some();

    let has_artist =
        from_tags.artist.is_some() || from_path.artist.is_some() || has_tag(TagField::Artist);
    let has_title =
        from_tags.title.is_some() || from_path.title.is_some() || has_tag(TagField::Title);
    let path_confidence = if has_artist && has_title && path_has_structure {
        0.94
    } else {
        0.78
    };

    let tag_year = from_tags.year.map(|y| y.to_string());
    let path_year = from_path.year.map(|y| y.to_string());
    let tag_title = from_tags
        .title
        .as_deref()
        .map(|title| title_with_mix(title, from_tags.mix.as_deref()));
    let path_title = from_path
        .title
        .as_deref()
        .map(|title| title_with_mix(title, from_path.mix.as_deref()));

    let tag_candidates = [
        (TagField::Artist, from_tags.artist.as_deref()),
        (TagField::Title, tag_title.as_deref()),
        (TagField::Album, from_tags.album.as_deref()),
        (TagField::Label, from_tags.label.as_deref()),
        (TagField::CatNo, from_tags.catno.as_deref()),
        (TagField::Year, tag_year.as_deref()),
        (TagField::TrackNumber, from_tags.track_number.as_deref()),
    ];

    let path_candidates = [
        (TagField::Artist, from_path.artist.as_deref()),
        (TagField::Title, path_title.as_deref()),
        (TagField::Album, from_path.album.as_deref()),
        (TagField::Label, from_path.label.as_deref()),
        (TagField::CatNo, from_path.catno.as_deref()),
        (TagField::Year, path_year.as_deref()),
        (TagField::TrackNumber, from_path.track_number.as_deref()),
    ];

    let mut proposals = Vec::new();
    for (field, raw) in tag_candidates {
        let Some(raw) = raw else { continue };
        let Ok(value) = normalise_value(field, raw) else {
            continue;
        };
        if !value.is_empty() {
            let confidence = if from_tags.artist_inferred_from_title
                && matches!(field, TagField::Artist | TagField::Title)
            {
                0.85
            } else {
                0.95
            };
            proposals.push(Proposal {
                field,
                value,
                provider: "tags".to_owned(),
                confidence,
                audio_matched: false,
            });
        }
    }

    if path_has_structure || !has_tag(TagField::Title) {
        for (field, raw) in path_candidates {
            if proposals.iter().any(|p| p.field == field) || has_tag(field) {
                continue;
            }
            let Some(raw) = raw else { continue };
            let Ok(value) = normalise_value(field, raw) else {
                continue;
            };
            if !value.is_empty() {
                proposals.push(Proposal {
                    field,
                    value,
                    provider: "filename".to_owned(),
                    confidence: path_confidence,
                    audio_matched: false,
                });
            }
        }
    }

    changing_proposals(proposals, file_tags)
}

/// Computes the `(field, new_value)` edits that Find and Replace makes on a track's effective
/// tags. When `fields` is empty, searches `BATCH_TEXT_FIELDS`.
pub fn find_and_replace_edits(
    effective: &[(TagField, String)],
    fields: &[TagField],
    find: &str,
    replace: &str,
    case_sensitive: bool,
) -> Vec<(TagField, String)> {
    let target_fields = if fields.is_empty() {
        &BATCH_TEXT_FIELDS[..]
    } else {
        fields
    };
    let mut edits = Vec::new();
    for &field in target_fields {
        let Some((_, current)) = effective.iter().find(|(held, _)| *held == field) else {
            continue;
        };
        let Some(replaced) = find_and_replace(current, find, replace, case_sensitive) else {
            continue;
        };
        let Ok(normalised) = normalise_value(field, &replaced) else {
            continue;
        };
        if normalised != *current {
            edits.push((field, normalised));
        }
    }
    edits
}

/// Computes the `(field, new_value)` edits that Fix Case makes on a track's effective tags.
/// When `fields` is empty, fixes `CASE_FIX_DEFAULT_FIELDS`.
pub fn fix_case_edits(
    effective: &[(TagField, String)],
    fields: &[TagField],
    case: TextCase,
) -> Vec<(TagField, String)> {
    let target_fields = if fields.is_empty() {
        &CASE_FIX_DEFAULT_FIELDS[..]
    } else {
        fields
    };
    let mut edits = Vec::new();
    for &field in target_fields {
        let Some((_, current)) = effective.iter().find(|(held, _)| *held == field) else {
            continue;
        };
        if current.trim().is_empty() {
            continue;
        }
        let fixed = fix_case(current, case);
        if !fixed.is_empty() && fixed != *current {
            edits.push((field, fixed));
        }
    }
    edits
}

/// Collects the release fields (`Album`, `Label`, `CatNo`, `Year` by default) from a reference
/// track's effective tags, falling back to its pending or accepted proposals when a field is not
/// yet in the effective layer.
pub fn release_edits_from(
    effective: &[(TagField, String)],
    proposals: &[StoredProposal],
    fields: &[TagField],
) -> Vec<(TagField, String)> {
    let target_fields = if fields.is_empty() {
        &RELEASE_FIELDS[..]
    } else {
        fields
    };
    let mut edits = Vec::new();
    for &field in target_fields {
        let from_effective = effective
            .iter()
            .find(|(held, value)| *held == field && !value.trim().is_empty())
            .map(|(_, value)| value.clone());
        let value = from_effective.or_else(|| {
            proposals
                .iter()
                .find(|stored| {
                    stored.proposal.field == field
                        && stored.status != ProposalStatus::Skipped
                        && !stored.proposal.value.trim().is_empty()
                })
                .map(|stored| stored.proposal.value.clone())
        });
        if let Some(value) = value
            && let Ok(normalised) = normalise_value(field, &value)
            && !normalised.is_empty()
        {
            edits.push((field, normalised));
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_online_match_puts_a_track_up_for_review() {
        assert_eq!(PrepStage::of(true, false, false), PrepStage::NotSearched);
        assert_eq!(PrepStage::of(true, true, false), PrepStage::NoMatch);
        assert_eq!(PrepStage::of(true, true, true), PrepStage::Review);
        assert_eq!(PrepStage::of(false, false, false), PrepStage::NotSearched);
        assert_eq!(PrepStage::of(false, true, false), PrepStage::NoMatch);
        assert_eq!(PrepStage::of(false, true, true), PrepStage::Done);
    }

    #[test]
    fn ordering_by_stage_groups_tracks_and_keeps_their_order_inside_a_group() {
        let stage_of = |id: i64| match id % 3 {
            0 => PrepStage::Review,
            1 => PrepStage::NotSearched,
            _ => PrepStage::Done,
        };
        let mut ids = vec![1, 2, 3, 4, 5, 6];
        order_by_stage(&mut ids, stage_of, SortDirection::Asc);
        assert_eq!(ids, [3, 6, 1, 4, 2, 5]);
        order_by_stage(&mut ids, stage_of, SortDirection::Desc);
        assert_eq!(ids, [2, 5, 1, 4, 3, 6]);
    }

    #[test]
    fn title_and_sentence_case_keep_dj_acronyms_and_roman_numerals_uppercase() {
        assert_eq!(
            fix_case(
                "dj shadow & mc dynamite - uk vip ep ii (extended mix)",
                TextCase::Title
            ),
            "DJ Shadow & MC Dynamite - UK VIP EP II (Extended Mix)"
        );
        assert_eq!(
            fix_case("sun and bass lp (feat. djs from the uk)", TextCase::Title),
            "Sun and Bass LP (feat. DJs from the UK)"
        );
        assert_eq!(
            fix_case(
                "DEEP ROLLING TECHNO FROM THE UK VIP EP II (FEAT. MC CONRAD)",
                TextCase::Sentence
            ),
            "Deep rolling techno from the UK VIP EP II (feat. MC conrad)"
        );
        assert_eq!(
            fix_case("Echo Chamber (Original Mix)", TextCase::Upper),
            "ECHO CHAMBER (ORIGINAL MIX)"
        );
    }

    #[test]
    fn find_and_replace_matches_case_insensitively_and_collapses_leftover_spaces() {
        assert_eq!(
            find_and_replace(
                "Echo Chamber (Original Mix) [www.promos.test]",
                "[WWW.PROMOS.TEST]",
                "",
                false
            ),
            Some("Echo Chamber (Original Mix)".to_owned())
        );
        assert_eq!(
            find_and_replace("Kaden - Echo Chamber", "echo", "Resonant", true),
            None
        );
        assert_eq!(
            find_and_replace("Kaden - Echo Chamber", "Echo", "Resonant", true),
            Some("Kaden - Resonant Chamber".to_owned())
        );
        assert_eq!(
            find_and_replace("Echo  Chamber (Original Mix)", "Original Mix", "", false),
            Some("Echo Chamber".to_owned())
        );
    }

    #[test]
    fn release_group_clusters_tracks_sharing_a_catalogue_number_album_or_folder() {
        let first = release_group(
            "[VEC001] Vector Recordings - Horizon EP (2024)/01 - Kaden - Echo Chamber.wav",
            Some("Horizon EP"),
            Some("Vector Recordings"),
            Some("VEC001"),
            Some("2024"),
        )
        .unwrap();
        let second = release_group(
            "[VEC001] Vector Recordings - Horizon EP (2024)/02 - Kaden - Night Drive.wav",
            Some("Horizon EP"),
            Some("Vector Recordings"),
            Some("VEC001"),
            Some("2024"),
        )
        .unwrap();
        assert_eq!(first.key, second.key);
        assert_eq!(first.title, "Horizon EP");
        assert_eq!(
            first.subtitle.as_deref(),
            Some("Vector Recordings · VEC001 · 2024")
        );
    }

    #[test]
    fn local_proposals_extract_normalised_tags_and_structured_filenames() {
        let untagged = local_proposals(
            &[],
            "[VEC001] Vector Recordings - Horizon EP (2024)/01 - Kaden - Echo Chamber (Extended Mix).wav",
        );
        assert!(
            untagged
                .iter()
                .all(|p| p.provider == "filename" && p.confidence >= 0.9)
        );
        assert!(
            untagged
                .iter()
                .any(|p| p.field == TagField::Artist && p.value == "Kaden")
        );
        assert!(
            untagged
                .iter()
                .any(|p| p.field == TagField::Title && p.value == "Echo Chamber (Extended Mix)")
        );
        assert!(untagged.iter().all(|p| p.field != TagField::Mix));
        assert!(
            untagged
                .iter()
                .any(|p| p.field == TagField::Album && p.value == "Horizon EP")
        );
        assert!(
            untagged
                .iter()
                .any(|p| p.field == TagField::Label && p.value == "Vector Recordings")
        );
        assert!(
            untagged
                .iter()
                .any(|p| p.field == TagField::CatNo && p.value == "VEC001")
        );
        assert!(
            untagged
                .iter()
                .any(|p| p.field == TagField::Year && p.value == "2024")
        );

        let already_clean = local_proposals(
            &[
                (TagField::Artist, "Some Artist".to_owned()),
                (TagField::Title, "Night Drive".to_owned()),
            ],
            "Some Artist - Night Drive.flac",
        );
        assert!(already_clean.is_empty());

        let edit_in_title = local_proposals(
            &[
                (TagField::Artist, "Some Artist".to_owned()),
                (TagField::Title, "Night Drive (Late Edit)".to_owned()),
            ],
            "Some Artist - Night Drive (Late Edit).mp3",
        );
        assert!(
            edit_in_title.is_empty(),
            "the edit stays in the title: {edit_in_title:?}"
        );

        let inferred = local_proposals(
            &[(TagField::Title, "Ada Marsh - Blue Hour".to_owned())],
            "track.mp3",
        );
        let artist_prop = inferred
            .iter()
            .find(|p| p.field == TagField::Artist)
            .expect("artist inferred from title");
        assert_eq!(artist_prop.value, "Ada Marsh");
        assert!(artist_prop.confidence < 0.90);
    }
}
