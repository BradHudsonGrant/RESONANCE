use std::collections::BTreeSet;

use crate::key::Key;
use crate::library::AudioFormat;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FilterPill {
    #[default]
    All,
    Favourites,
    Recent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TrackSortField {
    #[default]
    Index,
    Title,
    Artist,
    Bpm,
    Key,
    Duration,
    Added,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackQuery {
    pub text: Option<String>,
    pub pill: FilterPill,
    pub collection_id: Option<i64>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    pub keys: Vec<String>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub label: Option<String>,
    pub formats: Vec<AudioFormat>,
    pub sort: TrackSortField,
    pub direction: SortDirection,
}

impl TrackQuery {
    /// Whether any filter pill, collection, search text or drawer criterion narrows the library.
    pub fn is_narrowed(&self) -> bool {
        self.pill != FilterPill::All
            || self.collection_id.is_some()
            || build_fts_match_query(self.text.as_deref().unwrap_or("")).is_some()
            || self.active_filter_count() > 0
    }

    /// Number of active drawer criteria (`bpm`, `keys`, `year`, `label`, `formats`).
    pub fn active_filter_count(&self) -> usize {
        let mut count = 0;
        if self.bpm_min.is_some() || self.bpm_max.is_some() {
            count += 1;
        }
        if self.keys.iter().any(|key| !key.trim().is_empty()) {
            count += 1;
        }
        if self.year_min.is_some() || self.year_max.is_some() {
            count += 1;
        }
        if self
            .label
            .as_deref()
            .is_some_and(|label| !label.trim().is_empty())
        {
            count += 1;
        }
        if !self.formats.is_empty() {
            count += 1;
        }
        count
    }
}

/// Extract a 4-digit year from tags such as `"2021"` or `"2021-05-14"`.
pub fn parse_year_tag(raw: &str) -> Option<i32> {
    let trimmed = raw.trim();
    let head = trimmed.get(..4)?;
    if !head.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year: i32 = head.parse().ok()?;
    (1000..=2999).contains(&year).then_some(year)
}

const MAX_FTS_TOKENS: usize = 32;

/// Convert raw user search input into a safe FTS5 prefix `MATCH` expression.
///
/// Every token is alphanumeric and wrapped in double quotes with a trailing `*`, so FTS5 operators
/// (`AND`, `OR`, `NOT`, `NEAR`), punctuation, unbalanced quotes or column prefixes cannot error.
pub fn build_fts_match_query(raw: &str) -> Option<String> {
    let mut terms = Vec::new();
    let mut current = String::new();

    for ch in raw.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            terms.push(format!("\"{current}\"*"));
            current.clear();
            if terms.len() >= MAX_FTS_TOKENS {
                break;
            }
        }
    }
    if !current.is_empty() && terms.len() < MAX_FTS_TOKENS {
        terms.push(format!("\"{current}\"*"));
    }

    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Expand user-selected key names/codes into every equivalent spelling that stored tags may use.
pub fn key_filter_values(keys: &[String]) -> Vec<String> {
    let mut values = BTreeSet::new();
    for raw in keys {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(parsed) = trimmed.parse::<Key>() {
            values.extend(parsed.aliases());
        } else {
            values.insert(trimmed.to_owned());
        }
    }
    values.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_wraps_each_token_as_quoted_prefix() {
        assert_eq!(
            build_fts_match_query("daft pun").as_deref(),
            Some("\"daft\"* \"pun\"*")
        );
        assert_eq!(
            build_fts_match_query("  Tiësto ").as_deref(),
            Some("\"Tiësto\"*")
        );
    }

    #[test]
    fn fts_query_neutralises_operators_quotes_and_punctuation() {
        assert_eq!(
            build_fts_match_query("AND \"OR\" (not:near)*^").as_deref(),
            Some("\"AND\"* \"OR\"* \"not\"* \"near\"*")
        );
        assert_eq!(
            build_fts_match_query("don't-stop_the/beat").as_deref(),
            Some("\"don\"* \"t\"* \"stop\"* \"the\"* \"beat\"*")
        );
        assert_eq!(build_fts_match_query("   \"\" *** () :- ").as_deref(), None);
    }

    #[test]
    fn year_tag_parses_plain_years_and_iso_dates() {
        assert_eq!(parse_year_tag("2024"), Some(2024));
        assert_eq!(parse_year_tag(" 1998-05-14 "), Some(1998));
        assert_eq!(parse_year_tag("99"), None);
        assert_eq!(parse_year_tag("unknown"), None);
    }

    #[test]
    fn key_filter_expands_camelot_and_musical_without_major_minor_confusion() {
        let expanded = key_filter_values(&["8A".to_owned()]);
        assert!(expanded.iter().any(|value| value == "8A"));
        assert!(expanded.iter().any(|value| value == "Am"));
        assert!(expanded.iter().any(|value| value == "A minor"));
        assert!(!expanded.iter().any(|value| value == "AM"));

        let same_from_name = key_filter_values(&["Am".to_owned()]);
        assert_eq!(expanded, same_from_name);
    }
}
