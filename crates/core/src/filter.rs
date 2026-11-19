//! Filters on any column of the library table: which conditions suit which column, how a filter
//! reads to the user, its stable key, and the `/` filters the command bar takes.
//!
//! A query's filters are terms joined by AND. A term is one filter, or a group whose filters are
//! joined by OR. One level of grouping is all a DJ needs ("no genre or no label").

use std::fmt;

use crate::columns::{LibraryColumn, generic_tag_fields, tag_label, tag_sort_value};
use crate::key::Key;
use crate::library::AudioFormat;

/// The most filters one "any of" group holds.
pub const MAX_GROUP: usize = 8;
/// The most terms one query holds.
pub const MAX_TERMS: usize = 24;
/// The most values one "any of" or "none of" condition holds.
pub const MAX_VALUES: usize = 64;

/// What a column holds, which decides the conditions it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind {
    Text,
    Number,
    Date,
    Choice,
    Picture,
    /// Where the file is. Always known, and too long for "is".
    Location,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConditionKind {
    Contains,
    Is,
    IsNot,
    StartsWith,
    AnyOf,
    NoneOf,
    Range,
    Below,
    Above,
    Missing,
    Present,
}

impl ConditionKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Is => "is",
            Self::IsNot => "is_not",
            Self::StartsWith => "starts_with",
            Self::AnyOf => "any_of",
            Self::NoneOf => "none_of",
            Self::Range => "range",
            Self::Below => "below",
            Self::Above => "above",
            Self::Missing => "missing",
            Self::Present => "present",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        [
            Self::Contains,
            Self::Is,
            Self::IsNot,
            Self::StartsWith,
            Self::AnyOf,
            Self::NoneOf,
            Self::Range,
            Self::Below,
            Self::Above,
            Self::Missing,
            Self::Present,
        ]
        .into_iter()
        .find(|kind| kind.name() == name)
    }
}

/// What a filter asks of its column. Numbers are in the units [`FieldKind`] names: seconds for
/// Time, bytes for File size, hertz for Sample rate, and Unix milliseconds for dates.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Contains(String),
    Is(String),
    /// Tracks without a value pass, so hiding a genre never hides untagged tracks.
    IsNot(String),
    StartsWith(String),
    AnyOf(Vec<String>),
    /// Tracks without a value pass, as with [`Condition::IsNot`].
    NoneOf(Vec<String>),
    /// Inclusive at both ends. An open end has no bound.
    Range {
        min: Option<f64>,
        max: Option<f64>,
    },
    Below(f64),
    Above(f64),
    Missing,
    Present,
}

impl Condition {
    pub fn kind(&self) -> ConditionKind {
        match self {
            Self::Contains(_) => ConditionKind::Contains,
            Self::Is(_) => ConditionKind::Is,
            Self::IsNot(_) => ConditionKind::IsNot,
            Self::StartsWith(_) => ConditionKind::StartsWith,
            Self::AnyOf(_) => ConditionKind::AnyOf,
            Self::NoneOf(_) => ConditionKind::NoneOf,
            Self::Range { .. } => ConditionKind::Range,
            Self::Below(_) => ConditionKind::Below,
            Self::Above(_) => ConditionKind::Above,
            Self::Missing => ConditionKind::Missing,
            Self::Present => ConditionKind::Present,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    /// The column does not offer the condition.
    Unsupported {
        column: String,
        condition: ConditionKind,
    },
    /// A text or list condition with nothing in it.
    Empty,
    BadValue(String),
    UnknownColumn(String),
    /// A group with no filters or more than [`MAX_GROUP`].
    GroupSize,
    TooManyTerms,
    /// The command bar text is not a filter.
    NotAFilter,
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { column, condition } => write!(
                f,
                "{column} cannot be filtered with \"{}\".",
                condition.name().replace('_', " ")
            ),
            Self::Empty => f.write_str("The filter needs a value."),
            Self::BadValue(value) => write!(f, "\"{value}\" is not a value this filter takes."),
            Self::UnknownColumn(column) => write!(f, "There is no column called \"{column}\"."),
            Self::GroupSize => write!(f, "A group holds 1 to {MAX_GROUP} filters."),
            Self::TooManyTerms => write!(f, "A view holds at most {MAX_TERMS} filters."),
            Self::NotAFilter => f.write_str("That is not a filter."),
        }
    }
}

impl std::error::Error for FilterError {}

impl LibraryColumn {
    pub fn filter_kind(self) -> FieldKind {
        match self {
            Self::Title
            | Self::Artist
            | Self::Composer
            | Self::Remixer
            | Self::Mix
            | Self::Comment
            | Self::Album
            | Self::Genre
            | Self::Label
            | Self::CatNo
            | Self::Tag(_) => FieldKind::Text,
            Self::Bpm
            | Self::Time
            | Self::Year
            | Self::Bitrate
            | Self::SampleRate
            | Self::BitDepth
            | Self::Channels
            | Self::FileSize => FieldKind::Number,
            Self::Added | Self::Modified => FieldKind::Date,
            Self::Key | Self::Format => FieldKind::Choice,
            Self::Artwork => FieldKind::Picture,
            Self::Path | Self::Folder => FieldKind::Location,
        }
    }

    /// Whether a track can lack a value here, so "is missing" means something.
    pub fn can_be_missing(self) -> bool {
        !matches!(
            self,
            Self::Format
                | Self::FileSize
                | Self::Added
                | Self::Modified
                | Self::Path
                | Self::Folder
        )
    }

    /// The conditions the filter builder offers for the column, in the order it lists them.
    pub fn conditions(self) -> Vec<ConditionKind> {
        use ConditionKind as C;
        let mut kinds = match self.filter_kind() {
            FieldKind::Text => vec![
                C::Contains,
                C::Is,
                C::IsNot,
                C::StartsWith,
                C::AnyOf,
                C::NoneOf,
            ],
            FieldKind::Number | FieldKind::Date => vec![C::Range, C::Below, C::Above],
            FieldKind::Choice => vec![C::AnyOf, C::NoneOf],
            FieldKind::Picture => Vec::new(),
            FieldKind::Location => vec![C::Contains, C::StartsWith],
        };
        if self.can_be_missing() {
            kinds.extend([C::Missing, C::Present]);
        }
        kinds
    }

    /// The column's name inside a sentence: "genre", "catalogue number", but "BPM".
    pub fn noun(self) -> String {
        let label = match self {
            Self::Tag(field) => tag_label(field).to_owned(),
            Self::Added => "date added".to_owned(),
            Self::Modified => "date modified".to_owned(),
            other => other.label(),
        };
        lower_first(&label)
    }

    /// The column's name at the start of a filter.
    fn filter_label(self) -> String {
        match self {
            Self::Tag(field) => tag_label(field).to_owned(),
            other => other.label(),
        }
    }
}

/// Every column that can be filtered, in chooser order.
pub fn filterable_columns() -> Vec<LibraryColumn> {
    LibraryColumn::fixed()
        .iter()
        .copied()
        .chain(generic_tag_fields().map(LibraryColumn::Tag))
        .collect()
}

/// One condition on one column.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldFilter {
    pub column: LibraryColumn,
    pub condition: Condition,
}

impl FieldFilter {
    /// Checks the condition suits the column and tidies its values: trimmed text, keys in
    /// Camelot, known formats, ranges in order. Lists drop repeats.
    pub fn new(column: LibraryColumn, condition: Condition) -> Result<Self, FilterError> {
        let kind = condition.kind();
        if !column.conditions().contains(&kind) {
            return Err(FilterError::Unsupported {
                column: column.label(),
                condition: kind,
            });
        }
        let condition = match condition {
            Condition::Contains(text) => Condition::Contains(clean_text(&text)?),
            Condition::Is(text) => Condition::Is(clean_text(&text)?),
            Condition::IsNot(text) => Condition::IsNot(clean_text(&text)?),
            Condition::StartsWith(text) => Condition::StartsWith(clean_text(&text)?),
            Condition::AnyOf(values) => Condition::AnyOf(clean_values(column, &values)?),
            Condition::NoneOf(values) => Condition::NoneOf(clean_values(column, &values)?),
            Condition::Range { min, max } => {
                let (min, max) = match (finite(min)?, finite(max)?) {
                    (None, None) => return Err(FilterError::Empty),
                    (Some(low), Some(high)) if low > high => (Some(high), Some(low)),
                    bounds => bounds,
                };
                Condition::Range { min, max }
            }
            Condition::Below(value) => Condition::Below(finite_value(value)?),
            Condition::Above(value) => Condition::Above(finite_value(value)?),
            Condition::Missing => Condition::Missing,
            Condition::Present => Condition::Present,
        };
        Ok(Self { column, condition })
    }

    pub fn missing(column: LibraryColumn) -> Result<Self, FilterError> {
        Self::new(column, Condition::Missing)
    }

    /// How the filter reads in the bar, the drawer and the command bar.
    pub fn label(&self) -> String {
        let column = self.column;
        let name = column.filter_label();
        let shown = |value: &String| shown_value(column, value);
        match &self.condition {
            Condition::Missing => format!("No {}", column.noun()),
            Condition::Present => format!("Has {}", column.noun()),
            Condition::Contains(text) => format!("{name} contains {}", quoted(text)),
            Condition::Is(text) => format!("{name} is {}", quoted(text)),
            Condition::IsNot(text) => format!("{name} is not {}", quoted(text)),
            Condition::StartsWith(text) => format!("{name} starts with {}", quoted(text)),
            Condition::AnyOf(values) => {
                format!("{name} is {}", spoken_list(values.iter().map(shown), "or"))
            }
            Condition::NoneOf(values) if values.len() == 1 => {
                format!("{name} is not {}", shown(&values[0]))
            }
            Condition::NoneOf(values) => format!(
                "{name} is none of {}",
                values.iter().map(shown).collect::<Vec<_>>().join(", ")
            ),
            Condition::Range { min, max } => {
                let value = |number: f64| number_text(column, number);
                let date = column.filter_kind() == FieldKind::Date;
                match (min, max) {
                    (Some(low), Some(high)) if low == high => format!("{name} {}", value(*low)),
                    (Some(low), Some(high)) => format!("{name} {}–{}", value(*low), value(*high)),
                    (Some(low), None) if date => format!("{name} from {}", value(*low)),
                    (Some(low), None) => format!("{name} {} or more", value(*low)),
                    (None, Some(high)) if date => format!("{name} until {}", value(*high)),
                    (None, Some(high)) => format!("{name} {} or less", value(*high)),
                    (None, None) => name,
                }
            }
            Condition::Below(number) => {
                let word = if column.filter_kind() == FieldKind::Date {
                    "before"
                } else {
                    "under"
                };
                format!("{name} {word} {}", number_text(column, *number))
            }
            Condition::Above(number) => {
                let word = if column.filter_kind() == FieldKind::Date {
                    "after"
                } else {
                    "over"
                };
                format!("{name} {word} {}", number_text(column, *number))
            }
        }
    }

    /// A stable text form: the same filter always gives the same key, so a key names a filter
    /// in the usage counts, the quick filters and saved views.
    pub fn key(&self) -> String {
        let mut key = format!("{}:{}", self.column.id(), self.condition.kind().name());
        match &self.condition {
            Condition::Contains(text)
            | Condition::Is(text)
            | Condition::IsNot(text)
            | Condition::StartsWith(text) => {
                key.push(':');
                key.push_str(&escape(text));
            }
            Condition::AnyOf(values) | Condition::NoneOf(values) => {
                let mut sorted: Vec<String> = values.iter().map(|value| escape(value)).collect();
                sorted.sort_by_key(|value| value.to_lowercase());
                key.push(':');
                key.push_str(&sorted.join(","));
            }
            Condition::Range { min, max } => {
                let bound = |value: &Option<f64>| value.map(|number| number.to_string());
                key.push_str(&format!(
                    ":{},{}",
                    bound(min).unwrap_or_default(),
                    bound(max).unwrap_or_default()
                ));
            }
            Condition::Below(number) | Condition::Above(number) => {
                key.push_str(&format!(":{number}"));
            }
            Condition::Missing | Condition::Present => {}
        }
        key
    }

    pub fn from_key(key: &str) -> Result<Self, FilterError> {
        let mut parts = key.splitn(3, ':');
        let column_id = parts.next().unwrap_or_default();
        // A tag column's id has its own colon.
        let (column_id, op, rest) = if column_id == "tag" {
            let name = parts.next().unwrap_or_default();
            let tail = parts.next().unwrap_or_default();
            let mut tail = tail.splitn(2, ':');
            (
                format!("tag:{name}"),
                tail.next().unwrap_or_default().to_owned(),
                tail.next(),
            )
        } else {
            (
                column_id.to_owned(),
                parts.next().unwrap_or_default().to_owned(),
                parts.next(),
            )
        };
        let column = LibraryColumn::from_id(&column_id)
            .ok_or_else(|| FilterError::UnknownColumn(column_id.clone()))?;
        let kind =
            ConditionKind::from_name(&op).ok_or_else(|| FilterError::BadValue(op.clone()))?;
        let rest = rest.unwrap_or_default();
        let values = || -> Vec<String> {
            rest.split(',')
                .filter(|value| !value.is_empty())
                .map(unescape)
                .collect()
        };
        let number = |text: &str| -> Result<f64, FilterError> {
            text.parse::<f64>()
                .map_err(|_| FilterError::BadValue(text.to_owned()))
        };
        let condition = match kind {
            ConditionKind::Contains => Condition::Contains(unescape(rest)),
            ConditionKind::Is => Condition::Is(unescape(rest)),
            ConditionKind::IsNot => Condition::IsNot(unescape(rest)),
            ConditionKind::StartsWith => Condition::StartsWith(unescape(rest)),
            ConditionKind::AnyOf => Condition::AnyOf(values()),
            ConditionKind::NoneOf => Condition::NoneOf(values()),
            ConditionKind::Range => {
                let (low, high) = rest.split_once(',').unwrap_or((rest, ""));
                let bound = |text: &str| -> Result<Option<f64>, FilterError> {
                    if text.is_empty() {
                        Ok(None)
                    } else {
                        number(text).map(Some)
                    }
                };
                Condition::Range {
                    min: bound(low)?,
                    max: bound(high)?,
                }
            }
            ConditionKind::Below => Condition::Below(number(rest)?),
            ConditionKind::Above => Condition::Above(number(rest)?),
            ConditionKind::Missing => Condition::Missing,
            ConditionKind::Present => Condition::Present,
        };
        Self::new(column, condition)
    }

    /// Generic tags sort with a padded leading number, and their filters compare the same way.
    pub fn tag_value(text: &str) -> String {
        tag_sort_value(text).unwrap_or_default()
    }
}

/// One AND term of a query: a single filter, or an "any of these" group.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterTerm {
    any_of: Vec<FieldFilter>,
}

impl FilterTerm {
    pub fn one(filter: FieldFilter) -> Self {
        Self {
            any_of: vec![filter],
        }
    }

    /// A group, without repeats.
    pub fn any_of(filters: Vec<FieldFilter>) -> Result<Self, FilterError> {
        let mut kept: Vec<FieldFilter> = Vec::with_capacity(filters.len());
        for filter in filters {
            if !kept.iter().any(|existing| existing.key() == filter.key()) {
                kept.push(filter);
            }
        }
        if kept.is_empty() || kept.len() > MAX_GROUP {
            return Err(FilterError::GroupSize);
        }
        Ok(Self { any_of: kept })
    }

    pub fn filters(&self) -> &[FieldFilter] {
        &self.any_of
    }

    pub fn label(&self) -> String {
        let mut labels = self.any_of.iter().map(FieldFilter::label);
        let first = labels.next().unwrap_or_default();
        labels.fold(first, |text, next| {
            format!("{text} or {}", lower_first(&next))
        })
    }

    /// The group's filters' keys in order, so the same group in any order has one key.
    pub fn key(&self) -> String {
        let mut keys: Vec<String> = self.any_of.iter().map(FieldFilter::key).collect();
        keys.sort();
        keys.join("|")
    }

    pub fn from_key(key: &str) -> Result<Self, FilterError> {
        let filters = key
            .split('|')
            .map(FieldFilter::from_key)
            .collect::<Result<Vec<_>, _>>()?;
        Self::any_of(filters)
    }
}

/// The terms of a query, one key a line, for keeping a saved view.
pub fn terms_key(terms: &[FilterTerm]) -> String {
    terms
        .iter()
        .map(FilterTerm::key)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reads [`terms_key`] back, skipping any term this build does not know.
pub fn terms_from_key(key: &str) -> Vec<FilterTerm> {
    key.lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| FilterTerm::from_key(line).ok())
        .collect()
}

/// Checks a query's terms: at most [`MAX_TERMS`], without repeats.
pub fn clean_terms(terms: Vec<FilterTerm>) -> Result<Vec<FilterTerm>, FilterError> {
    let mut kept: Vec<FilterTerm> = Vec::with_capacity(terms.len());
    for term in terms {
        if !kept.iter().any(|existing| existing.key() == term.key()) {
            kept.push(term);
        }
    }
    if kept.len() > MAX_TERMS {
        return Err(FilterError::TooManyTerms);
    }
    Ok(kept)
}

/// Reads a command bar filter such as `/missing genre`, `/bitrate < 320`, `/bpm 125-132` or
/// `/format flac, wav`. The leading slash is optional.
pub fn parse_command(text: &str) -> Result<FilterTerm, FilterError> {
    let text = text.trim().trim_start_matches('/').trim();
    let lower = text.to_lowercase();

    for (words, missing) in [
        ("missing ", true),
        ("no ", true),
        ("without ", true),
        ("has ", false),
        ("with ", false),
    ] {
        if let Some(rest) = lower.strip_prefix(words) {
            let (column, tail) = column_at_start(rest).ok_or(FilterError::NotAFilter)?;
            if !tail.trim().is_empty() {
                return Err(FilterError::NotAFilter);
            }
            let condition = if missing {
                Condition::Missing
            } else {
                Condition::Present
            };
            return FieldFilter::new(column, condition).map(FilterTerm::one);
        }
    }

    let (column, tail) = column_at_start(&lower).ok_or(FilterError::NotAFilter)?;
    // Column names are ASCII, so the name is as long in the text as in its lower case.
    let rest = text
        .get(lower.len() - tail.len()..)
        .unwrap_or_default()
        .trim();
    let condition = condition_from_words(column, rest)?;
    FieldFilter::new(column, condition).map(FilterTerm::one)
}

fn condition_from_words(column: LibraryColumn, rest: &str) -> Result<Condition, FilterError> {
    let lower = rest.to_lowercase();
    let kind = column.filter_kind();
    match lower.as_str() {
        "missing" | "is missing" | "empty" | "is empty" | "none" => return Ok(Condition::Missing),
        "present" | "is present" | "set" => return Ok(Condition::Present),
        "" => return Err(FilterError::Empty),
        _ => {}
    }

    const OPERATORS: [&str; 18] = [
        "<=",
        ">=",
        "!=",
        "<",
        ">",
        "=",
        "is not ",
        "isn't ",
        "not ",
        "is ",
        "contains ",
        "starts with ",
        "before ",
        "after ",
        "under ",
        "over ",
        "below ",
        "above ",
    ];
    let found = OPERATORS
        .iter()
        .find(|operator| lower.starts_with(*operator))
        .copied();
    let value = found.map_or(rest, |operator| {
        rest.get(operator.len()..).unwrap_or_default()
    });
    let value = value.trim();
    let list = || -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let number = || number_from_words(column, value);
    let numeric = matches!(kind, FieldKind::Number | FieldKind::Date);

    Ok(match (found.map(str::trim), kind) {
        (Some("<="), _) if numeric => Condition::Range {
            min: None,
            max: Some(number()?),
        },
        (Some(">="), _) if numeric => Condition::Range {
            min: Some(number()?),
            max: None,
        },
        (Some("<" | "before" | "under" | "below"), _) if numeric => Condition::Below(number()?),
        (Some(">" | "after" | "over" | "above"), _) if numeric => Condition::Above(number()?),
        (Some("=" | "is") | None, _) if numeric => {
            if let Some((low, high)) = split_range(value) {
                Condition::Range {
                    min: Some(number_from_words(column, low)?),
                    max: Some(number_from_words(column, high)?),
                }
            } else {
                let exact = number()?;
                Condition::Range {
                    min: Some(exact),
                    max: Some(exact),
                }
            }
        }
        (Some("contains"), _) => Condition::Contains(value.to_owned()),
        (Some("starts with"), _) => Condition::StartsWith(value.to_owned()),
        (Some("=" | "is") | None, FieldKind::Choice) => Condition::AnyOf(list()),
        (Some("!=" | "is not" | "isn't" | "not"), FieldKind::Choice) => Condition::NoneOf(list()),
        (Some("=" | "is"), FieldKind::Text) if list().len() > 1 => Condition::AnyOf(list()),
        (Some("=" | "is"), FieldKind::Text) => Condition::Is(value.to_owned()),
        (Some("!=" | "is not" | "isn't" | "not"), FieldKind::Text) if list().len() > 1 => {
            Condition::NoneOf(list())
        }
        (Some("!=" | "is not" | "isn't" | "not"), FieldKind::Text) => {
            Condition::IsNot(value.to_owned())
        }
        (None, FieldKind::Text | FieldKind::Location) => Condition::Contains(value.to_owned()),
        _ => return Err(FilterError::NotAFilter),
    })
}

fn split_range(value: &str) -> Option<(&str, &str)> {
    let (low, high) = value
        .split_once(" to ")
        .or_else(|| value.split_once('–'))
        .or_else(|| {
            // A dash between two numbers, not the one inside a date.
            let at = value.find('-')?;
            (at > 0 && !value.contains(':') && value.matches('-').count() == 1)
                .then(|| value.split_at(at))
                .map(|(low, high)| (low, &high[1..]))
        })?;
    let (low, high) = (low.trim(), high.trim());
    (!low.is_empty() && !high.is_empty()).then_some((low, high))
}

/// A number as the user types it for the column: `5:30` for a time, `320k` for a bitrate,
/// `44.1k` for a sample rate, `20mb` for a file size, `2024-05-01` for a date.
fn number_from_words(column: LibraryColumn, text: &str) -> Result<f64, FilterError> {
    let lower = text.trim().to_lowercase();
    let bad = || FilterError::BadValue(text.trim().to_owned());
    if column.filter_kind() == FieldKind::Date {
        return date_ms(&lower).ok_or_else(bad);
    }
    if column == LibraryColumn::Time
        && let Some((minutes, seconds)) = lower.split_once(':')
    {
        let minutes: f64 = minutes.trim().parse().map_err(|_| bad())?;
        let seconds: f64 = seconds.trim().parse().map_err(|_| bad())?;
        return Ok(minutes * 60.0 + seconds);
    }
    let digits_end = lower
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .unwrap_or(lower.len());
    let (digits, unit) = lower.split_at(digits_end);
    let value: f64 = digits.parse().map_err(|_| bad())?;
    let unit = unit.trim();
    let scaled = match column {
        LibraryColumn::Bitrate if matches!(unit, "" | "k" | "kbps" | "kb/s") => value,
        LibraryColumn::SampleRate if matches!(unit, "k" | "khz") => value * 1000.0,
        LibraryColumn::SampleRate if matches!(unit, "" | "hz") => {
            if value < 1000.0 {
                value * 1000.0
            } else {
                value
            }
        }
        LibraryColumn::FileSize => match unit {
            "" | "mb" | "m" => value * 1_000_000.0,
            "kb" | "k" => value * 1000.0,
            "gb" | "g" => value * 1_000_000_000.0,
            "b" => value,
            _ => return Err(bad()),
        },
        LibraryColumn::Time if matches!(unit, "m" | "min") => value * 60.0,
        LibraryColumn::Time if matches!(unit, "" | "s") => value,
        LibraryColumn::BitDepth if matches!(unit, "" | "bit" | "-bit") => value,
        _ if unit.is_empty() => value,
        _ => return Err(bad()),
    };
    Ok(scaled)
}

/// The words for each filterable column the command bar knows, longest first.
fn column_names() -> Vec<(String, LibraryColumn)> {
    let mut names: Vec<(String, LibraryColumn)> = Vec::new();
    for column in filterable_columns() {
        names.push((column.noun().to_lowercase(), column));
        names.push((column.id().replace('_', " "), column));
        names.push((column.id().replace('_', ""), column));
        if let LibraryColumn::Tag(field) = column {
            names.push((tag_label(field).to_lowercase(), column));
        }
    }
    for (alias, column) in [
        ("tempo", LibraryColumn::Bpm),
        ("length", LibraryColumn::Time),
        ("duration", LibraryColumn::Time),
        ("cover", LibraryColumn::Artwork),
        ("art", LibraryColumn::Artwork),
        ("size", LibraryColumn::FileSize),
        ("catalogue", LibraryColumn::CatNo),
        ("catalog number", LibraryColumn::CatNo),
        ("cat no", LibraryColumn::CatNo),
        ("added", LibraryColumn::Added),
        ("modified", LibraryColumn::Modified),
        ("rate", LibraryColumn::SampleRate),
        ("samplerate", LibraryColumn::SampleRate),
        ("depth", LibraryColumn::BitDepth),
        ("bitdepth", LibraryColumn::BitDepth),
    ] {
        names.push((alias.to_owned(), column));
    }
    names.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
    names
}

/// The column named at the start of `lower`, and what follows it.
fn column_at_start(lower: &str) -> Option<(LibraryColumn, &str)> {
    column_names().into_iter().find_map(|(name, column)| {
        let tail = lower.strip_prefix(name.as_str())?;
        let ends_word = tail
            .chars()
            .next()
            .is_none_or(|ch| ch.is_whitespace() || "<>=!".contains(ch));
        ends_word.then_some((column, tail))
    })
}

fn clean_text(text: &str) -> Result<String, FilterError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Err(FilterError::Empty)
    } else {
        Ok(trimmed.to_owned())
    }
}

fn clean_values(column: LibraryColumn, values: &[String]) -> Result<Vec<String>, FilterError> {
    let mut kept: Vec<String> = Vec::new();
    for raw in values {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = match column {
            LibraryColumn::Format => {
                let lower = trimmed.to_ascii_lowercase();
                let format = AudioFormat::from_name(&lower)
                    .or_else(|| AudioFormat::from_extension(&lower))
                    .ok_or_else(|| FilterError::BadValue(trimmed.to_owned()))?;
                format.name().to_owned()
            }
            LibraryColumn::Key => trimmed
                .parse::<Key>()
                .map_err(|_| FilterError::BadValue(trimmed.to_owned()))?
                .camelot(),
            _ => trimmed.to_owned(),
        };
        if !kept
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&value))
        {
            kept.push(value);
        }
    }
    if kept.is_empty() {
        return Err(FilterError::Empty);
    }
    if kept.len() > MAX_VALUES {
        return Err(FilterError::BadValue(format!("{} values", kept.len())));
    }
    Ok(kept)
}

fn finite(value: Option<f64>) -> Result<Option<f64>, FilterError> {
    value.map(finite_value).transpose()
}

fn finite_value(value: f64) -> Result<f64, FilterError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(FilterError::BadValue(value.to_string()))
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    // "BPM" stays as it is.
    if chars.clone().next().is_some_and(char::is_uppercase) {
        return text.to_owned();
    }
    first.to_lowercase().chain(chars).collect()
}

fn quoted(text: &str) -> String {
    format!("\u{201c}{text}\u{201d}")
}

fn shown_value(column: LibraryColumn, value: &str) -> String {
    match column {
        LibraryColumn::Format => value.to_uppercase(),
        LibraryColumn::Key => value.to_owned(),
        _ => quoted(value),
    }
}

fn spoken_list(items: impl Iterator<Item = String>, last_word: &str) -> String {
    let items: Vec<String> = items.collect();
    match items.as_slice() {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} {last_word} {last}", rest.join(", ")),
    }
}

/// A number as the column shows it.
fn number_text(column: LibraryColumn, value: f64) -> String {
    let plain = |number: f64| {
        if number.fract() == 0.0 {
            format!("{number:.0}")
        } else {
            let text = format!("{number:.2}");
            text.trim_end_matches('0').to_owned()
        }
    };
    match column {
        LibraryColumn::Time => {
            let total = value.max(0.0).round() as u64;
            format!("{}:{:02}", total / 60, total % 60)
        }
        LibraryColumn::Bitrate => format!("{} kbps", plain(value)),
        LibraryColumn::SampleRate => format!("{} kHz", plain(value / 1000.0)),
        LibraryColumn::BitDepth => format!("{}-bit", plain(value)),
        LibraryColumn::FileSize => format!("{} MB", plain((value / 100_000.0).round() / 10.0)),
        LibraryColumn::Added | LibraryColumn::Modified => date_text(value),
        _ => plain(value),
    }
}

const DAY_MS: i64 = 86_400_000;

/// Unix milliseconds at the start (UTC) of `2024`, `2024-05` or `2024-05-01`.
fn date_ms(text: &str) -> Option<f64> {
    let mut parts = text.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next().map_or(Some(1), |part| part.parse().ok())?;
    let day: u32 = parts.next().map_or(Some(1), |part| part.parse().ok())?;
    if parts.next().is_some()
        || !(1000..=2999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
    {
        return None;
    }
    Some((days_from_civil(year, month, day) * DAY_MS) as f64)
}

fn date_text(ms: f64) -> String {
    let days = (ms as i64).div_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

// Howard Hinnant's day count for the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '%' => out.push_str("%25"),
            ':' => out.push_str("%3A"),
            ',' => out.push_str("%2C"),
            '|' => out.push_str("%7C"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            other => out.push(other),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        let code = rest.get(at + 1..at + 3);
        match code.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
            Some(byte) => {
                out.push(char::from(byte));
                rest = &rest[at + 3..];
            }
            None => {
                out.push('%');
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TagField;

    fn filter(column: LibraryColumn, condition: Condition) -> FieldFilter {
        FieldFilter::new(column, condition).unwrap()
    }

    #[test]
    fn each_kind_of_column_offers_the_conditions_that_suit_it() {
        use ConditionKind as C;
        assert_eq!(
            LibraryColumn::Genre.conditions(),
            [
                C::Contains,
                C::Is,
                C::IsNot,
                C::StartsWith,
                C::AnyOf,
                C::NoneOf,
                C::Missing,
                C::Present
            ]
        );
        assert_eq!(
            LibraryColumn::Bitrate.conditions(),
            [C::Range, C::Below, C::Above, C::Missing, C::Present]
        );
        assert_eq!(
            LibraryColumn::Added.conditions(),
            [C::Range, C::Below, C::Above]
        );
        assert_eq!(LibraryColumn::Format.conditions(), [C::AnyOf, C::NoneOf]);
        assert_eq!(
            LibraryColumn::Artwork.conditions(),
            [C::Missing, C::Present]
        );
        assert_eq!(
            LibraryColumn::Path.conditions(),
            [C::Contains, C::StartsWith]
        );
        assert_eq!(filterable_columns().len(), LibraryColumn::fixed().len() + 3);
    }

    #[test]
    fn a_condition_the_column_does_not_offer_is_refused() {
        assert!(matches!(
            FieldFilter::new(LibraryColumn::Format, Condition::Missing),
            Err(FilterError::Unsupported { .. })
        ));
        assert!(matches!(
            FieldFilter::new(LibraryColumn::Bpm, Condition::Contains("12".into())),
            Err(FilterError::Unsupported { .. })
        ));
        assert_eq!(
            FieldFilter::new(LibraryColumn::Genre, Condition::Is("  ".into())),
            Err(FilterError::Empty)
        );
        assert_eq!(
            FieldFilter::new(
                LibraryColumn::Bpm,
                Condition::Range {
                    min: None,
                    max: None
                }
            ),
            Err(FilterError::Empty)
        );
    }

    #[test]
    fn values_are_tidied_into_one_form() {
        let keys = filter(
            LibraryColumn::Key,
            Condition::AnyOf(vec!["Am".into(), "8A".into(), " C ".into()]),
        );
        assert_eq!(
            keys.condition,
            Condition::AnyOf(vec!["8A".into(), "8B".into()])
        );
        let formats = filter(
            LibraryColumn::Format,
            Condition::AnyOf(vec!["FLAC".into(), "aif".into()]),
        );
        assert_eq!(
            formats.condition,
            Condition::AnyOf(vec!["flac".into(), "aiff".into()])
        );
        let range = filter(
            LibraryColumn::Bpm,
            Condition::Range {
                min: Some(130.0),
                max: Some(120.0),
            },
        );
        assert_eq!(
            range.condition,
            Condition::Range {
                min: Some(120.0),
                max: Some(130.0)
            }
        );
    }

    #[test]
    fn filters_read_as_plain_sentences() {
        let label = |column, condition| filter(column, condition).label();
        assert_eq!(label(LibraryColumn::Genre, Condition::Missing), "No genre");
        assert_eq!(
            label(LibraryColumn::Artwork, Condition::Missing),
            "No artwork"
        );
        assert_eq!(label(LibraryColumn::Bpm, Condition::Present), "Has BPM");
        assert_eq!(
            label(LibraryColumn::CatNo, Condition::Missing),
            "No catalogue number"
        );
        assert_eq!(
            label(LibraryColumn::Label, Condition::IsNot("Toolroom".into())),
            "Label is not \u{201c}Toolroom\u{201d}"
        );
        assert_eq!(
            label(LibraryColumn::Bitrate, Condition::Below(320.0)),
            "Bitrate under 320 kbps"
        );
        assert_eq!(
            label(
                LibraryColumn::Format,
                Condition::AnyOf(vec!["flac".into(), "wav".into(), "aiff".into()])
            ),
            "Format is FLAC, WAV or AIFF"
        );
        assert_eq!(
            label(
                LibraryColumn::Bpm,
                Condition::Range {
                    min: Some(125.0),
                    max: Some(132.0)
                }
            ),
            "BPM 125–132"
        );
        assert_eq!(
            label(LibraryColumn::Time, Condition::Above(330.0)),
            "Time over 5:30"
        );
        assert_eq!(
            label(LibraryColumn::Added, Condition::Below(1_767_225_600_000.0)),
            "Date added before 2026-01-01"
        );
        assert_eq!(
            label(LibraryColumn::SampleRate, Condition::Above(44_100.0)),
            "Sample rate over 44.1 kHz"
        );
        assert_eq!(
            label(
                LibraryColumn::Tag(TagField::AlbumArtist),
                Condition::Missing
            ),
            "No album artist"
        );

        let group = FilterTerm::any_of(vec![
            filter(LibraryColumn::Genre, Condition::Missing),
            filter(LibraryColumn::Label, Condition::Missing),
        ])
        .unwrap();
        assert_eq!(group.label(), "No genre or no label");
    }

    #[test]
    fn a_key_reads_back_as_the_same_filter() {
        let filters = [
            filter(
                LibraryColumn::Comment,
                Condition::Contains("a,b:c|d%e\nf".into()),
            ),
            filter(
                LibraryColumn::Bpm,
                Condition::Range {
                    min: Some(124.5),
                    max: None,
                },
            ),
            filter(LibraryColumn::Bitrate, Condition::Below(320.0)),
            filter(
                LibraryColumn::Key,
                Condition::NoneOf(vec!["8A".into(), "9B".into()]),
            ),
            filter(
                LibraryColumn::Tag(TagField::TrackNumber),
                Condition::Is("7".into()),
            ),
            filter(LibraryColumn::Artwork, Condition::Present),
        ];
        for original in filters {
            let key = original.key();
            assert_eq!(FieldFilter::from_key(&key).unwrap(), original, "{key}");
        }

        let terms = vec![
            FilterTerm::one(filter(LibraryColumn::Genre, Condition::Missing)),
            FilterTerm::any_of(vec![
                filter(LibraryColumn::Label, Condition::Missing),
                filter(LibraryColumn::Year, Condition::Below(2000.0)),
            ])
            .unwrap(),
        ];
        assert_eq!(terms_from_key(&terms_key(&terms)), terms);
        assert!(terms_from_key("tension:missing\ngenre:missing").len() == 1);
    }

    #[test]
    fn the_same_filters_in_another_order_have_one_key() {
        let a = FilterTerm::any_of(vec![
            filter(LibraryColumn::Genre, Condition::Missing),
            filter(LibraryColumn::Label, Condition::Missing),
        ])
        .unwrap();
        let b = FilterTerm::any_of(vec![
            filter(LibraryColumn::Label, Condition::Missing),
            filter(LibraryColumn::Genre, Condition::Missing),
        ])
        .unwrap();
        assert_eq!(a.key(), b.key());
        let formats = |values: [&str; 2]| {
            filter(
                LibraryColumn::Format,
                Condition::AnyOf(values.map(str::to_owned).to_vec()),
            )
            .key()
        };
        assert_eq!(formats(["wav", "flac"]), formats(["flac", "wav"]));
    }

    #[test]
    fn command_bar_filters_read_into_conditions() {
        let parsed = |text: &str| parse_command(text).unwrap().filters()[0].clone();
        assert_eq!(
            parsed("/missing genre"),
            filter(LibraryColumn::Genre, Condition::Missing)
        );
        assert_eq!(
            parsed("/no artwork"),
            filter(LibraryColumn::Artwork, Condition::Missing)
        );
        assert_eq!(
            parsed("/has catalogue number"),
            filter(LibraryColumn::CatNo, Condition::Present)
        );
        assert_eq!(
            parsed("/bitrate < 320"),
            filter(LibraryColumn::Bitrate, Condition::Below(320.0))
        );
        assert_eq!(
            parsed("/format flac"),
            filter(LibraryColumn::Format, Condition::AnyOf(vec!["flac".into()]))
        );
        assert_eq!(
            parsed("/format flac, wav"),
            filter(
                LibraryColumn::Format,
                Condition::AnyOf(vec!["flac".into(), "wav".into()])
            )
        );
        assert_eq!(
            parsed("/bpm 125-132"),
            filter(
                LibraryColumn::Bpm,
                Condition::Range {
                    min: Some(125.0),
                    max: Some(132.0)
                }
            )
        );
        assert_eq!(
            parsed("/year >= 2020"),
            filter(
                LibraryColumn::Year,
                Condition::Range {
                    min: Some(2020.0),
                    max: None
                }
            )
        );
        assert_eq!(
            parsed("/label is Anjunadeep"),
            filter(LibraryColumn::Label, Condition::Is("Anjunadeep".into()))
        );
        assert_eq!(
            parsed("/genre not Techno"),
            filter(LibraryColumn::Genre, Condition::IsNot("Techno".into()))
        );
        assert_eq!(
            parsed("/artist deep"),
            filter(LibraryColumn::Artist, Condition::Contains("deep".into()))
        );
        assert_eq!(
            parsed("/time > 5:30"),
            filter(LibraryColumn::Time, Condition::Above(330.0))
        );
        assert_eq!(
            parsed("/sample rate >= 44.1k"),
            filter(
                LibraryColumn::SampleRate,
                Condition::Range {
                    min: Some(44_100.0),
                    max: None
                }
            )
        );
        assert_eq!(
            parsed("/size > 20mb"),
            filter(LibraryColumn::FileSize, Condition::Above(20_000_000.0))
        );
        assert_eq!(
            parsed("/added after 2026-01-01"),
            filter(LibraryColumn::Added, Condition::Above(1_767_225_600_000.0))
        );
        assert_eq!(
            parsed("/album artist missing"),
            filter(
                LibraryColumn::Tag(TagField::AlbumArtist),
                Condition::Missing
            )
        );
        assert_eq!(
            parsed("/key 8A, 9A"),
            filter(
                LibraryColumn::Key,
                Condition::AnyOf(vec!["8A".into(), "9A".into()])
            )
        );
    }

    #[test]
    fn command_bar_text_that_is_not_a_filter_is_refused() {
        assert_eq!(parse_command("/build set"), Err(FilterError::NotAFilter));
        assert_eq!(
            parse_command("/missing tension"),
            Err(FilterError::NotAFilter)
        );
        assert_eq!(parse_command("/bpm"), Err(FilterError::Empty));
        assert!(matches!(
            parse_command("/bitrate fast"),
            Err(FilterError::BadValue(_))
        ));
        assert!(matches!(
            parse_command("/missing format"),
            Err(FilterError::Unsupported { .. })
        ));
    }

    #[test]
    fn dates_round_trip_through_the_calendar() {
        for (text, expected) in [
            ("1970-01-01", 0.0),
            ("2000-03-01", 951_868_800_000.0),
            ("2026-01-01", 1_767_225_600_000.0),
        ] {
            assert_eq!(date_ms(text), Some(expected));
            assert_eq!(date_text(expected), text);
        }
        assert_eq!(date_ms("2024"), date_ms("2024-01-01"));
        assert_eq!(date_ms("2024-13-01"), None);
    }

    #[test]
    fn terms_drop_repeats_and_hold_a_limit() {
        let missing = FilterTerm::one(filter(LibraryColumn::Genre, Condition::Missing));
        assert_eq!(
            clean_terms(vec![missing.clone(), missing.clone()]).unwrap(),
            std::slice::from_ref(&missing)
        );
        let many: Vec<FilterTerm> = (0..=MAX_TERMS)
            .map(|at| {
                FilterTerm::one(filter(
                    LibraryColumn::Bpm,
                    Condition::Above(f64::from(at as u32)),
                ))
            })
            .collect();
        assert_eq!(clean_terms(many), Err(FilterError::TooManyTerms));
        assert_eq!(FilterTerm::any_of(Vec::new()), Err(FilterError::GroupSize));
    }
}
