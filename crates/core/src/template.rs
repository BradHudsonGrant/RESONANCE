//! Filename templates such as `{artist} - {title} ({mix})`, and the rules that turn one into a
//! name every platform accepts.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::library::TagField;

pub const DEFAULT_TEMPLATE: &str = "{artist} - {title} ({mix})";

/// The longest file name, in bytes, that ext4, APFS and NTFS all accept.
pub const MAX_NAME_BYTES: usize = 255;

/// Windows software that does not opt into long paths stops at 260 UTF-16 units with the
/// terminator, and DJ software on Windows is among it. A library moves between platforms, so
/// every rename keeps to that.
pub const MAX_PATH_UNITS: usize = 259;

/// Tokens a template can use, each read from one tag.
pub const TOKENS: [(&str, TagField); 9] = [
    ("artist", TagField::Artist),
    ("title", TagField::Title),
    ("mix", TagField::Mix),
    ("bpm", TagField::Bpm),
    ("key", TagField::Key),
    ("label", TagField::Label),
    ("catno", TagField::CatNo),
    ("year", TagField::Year),
    ("track", TagField::TrackNumber),
];

/// A file is not renamed when one of these is used and empty, because the name would not say
/// what the track is.
const REQUIRED: [TagField; 2] = [TagField::Artist, TagField::Title];

const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    Empty,
    Unclosed { at: usize },
    UnknownToken { token: String },
    NoToken,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "The template is empty."),
            Self::Unclosed { at } => {
                write!(f, "The {{ at character {} has no closing }}.", at + 1)
            }
            Self::UnknownToken { token } => write!(
                f,
                "{{{token}}} is not a token. Use {}.",
                TOKENS
                    .iter()
                    .map(|(name, _)| format!("{{{name}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::NoToken => write!(
                f,
                "The template has no tokens, so every file would get the same name."
            ),
        }
    }
}

impl std::error::Error for TemplateError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    Char(char),
    Token(TagField),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pieces: Vec<Piece>,
}

/// A name built from a track's tags, without the extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rendered {
    Stem(String),
    /// Required tags the template uses are empty, or nothing at all was left.
    Missing(Vec<TagField>),
}

impl Template {
    pub fn parse(text: &str) -> Result<Self, TemplateError> {
        if text.trim().is_empty() {
            return Err(TemplateError::Empty);
        }
        let mut pieces = Vec::new();
        let mut chars = text.char_indices();
        while let Some((at, c)) = chars.next() {
            if c != '{' {
                pieces.push(Piece::Char(c));
                continue;
            }
            let mut name = String::new();
            let mut closed = false;
            for (_, c) in chars.by_ref() {
                if c == '}' {
                    closed = true;
                    break;
                }
                name.push(c);
            }
            if !closed {
                return Err(TemplateError::Unclosed { at });
            }
            let field = TOKENS
                .iter()
                .find(|(token, _)| token.eq_ignore_ascii_case(name.trim()))
                .map(|(_, field)| *field)
                .ok_or(TemplateError::UnknownToken { token: name })?;
            pieces.push(Piece::Token(field));
        }
        if !pieces.iter().any(|piece| matches!(piece, Piece::Token(_))) {
            return Err(TemplateError::NoToken);
        }
        Ok(Self { pieces })
    }

    pub fn fields(&self) -> impl Iterator<Item = TagField> + '_ {
        self.pieces.iter().filter_map(|piece| match piece {
            Piece::Token(field) => Some(*field),
            Piece::Char(_) => None,
        })
    }

    /// Fills the template from `value`. Brackets around tokens that are all empty go, and so does
    /// the separator an empty token leaves behind, so `{artist} - {title} ({mix})` with no mix
    /// gives `Artist - Title`.
    pub fn render<'a>(&self, value: impl Fn(TagField) -> Option<&'a str>) -> Rendered {
        let values: Vec<Option<String>> = self
            .pieces
            .iter()
            .map(|piece| match piece {
                Piece::Token(field) => value(*field)
                    .and_then(|raw| token_value(*field, raw))
                    .filter(|text| !text.is_empty()),
                Piece::Char(_) => None,
            })
            .collect();

        let missing: Vec<TagField> = REQUIRED
            .into_iter()
            .filter(|required| {
                self.pieces
                    .iter()
                    .zip(&values)
                    .any(|(piece, value)| *piece == Piece::Token(*required) && value.is_none())
            })
            .collect();
        if !missing.is_empty() {
            return Rendered::Missing(missing);
        }

        let kept = self.drop_empty_brackets(&values);
        let mut segments = Vec::new();
        for (index, piece) in self.pieces.iter().enumerate() {
            if !kept[index] {
                continue;
            }
            match (piece, &values[index]) {
                (Piece::Char(c), _) => match segments.last_mut() {
                    Some(Segment::Literal(text)) => text.push(*c),
                    _ => segments.push(Segment::Literal(c.to_string())),
                },
                (Piece::Token(_), Some(text)) => segments.push(Segment::Value(sanitise(text))),
                (Piece::Token(_), None) => segments.push(Segment::Empty),
            }
        }
        let stem = finish_stem(&join_segments(segments));
        if stem.is_empty() {
            return Rendered::Missing(self.fields().collect());
        }
        Rendered::Stem(stem)
    }

    /// Marks which pieces stay once every bracket pair whose tokens are all empty is removed.
    fn drop_empty_brackets(&self, values: &[Option<String>]) -> Vec<bool> {
        let mut kept = vec![true; self.pieces.len()];
        let mut open: Vec<(char, usize)> = Vec::new();
        for (index, piece) in self.pieces.iter().enumerate() {
            let Piece::Char(c) = piece else { continue };
            match c {
                '(' | '[' => open.push((*c, index)),
                ')' | ']' => {
                    let opener = if *c == ')' { '(' } else { '[' };
                    let Some(position) = open.iter().rposition(|(o, _)| *o == opener) else {
                        continue;
                    };
                    let (_, start) = open[position];
                    open.truncate(position);
                    let inside = start + 1..index;
                    let mut tokens = self.pieces[inside.clone()]
                        .iter()
                        .zip(&values[inside])
                        .filter(|(piece, _)| matches!(piece, Piece::Token(_)))
                        .peekable();
                    if tokens.peek().is_some() && tokens.all(|(_, value)| value.is_none()) {
                        kept[start..=index].fill(false);
                    }
                }
                _ => {}
            }
        }
        kept
    }
}

enum Segment {
    Literal(String),
    Value(String),
    Empty,
}

fn is_separator_char(c: char) -> bool {
    c.is_whitespace() || matches!(c, '-' | '_' | '.' | ',' | '&' | '+' | '~')
}

fn separator_tail(text: &str) -> usize {
    text.len() - text.trim_end_matches(is_separator_char).len()
}

fn separator_head(text: &str) -> usize {
    text.len() - text.trim_start_matches(is_separator_char).len()
}

fn visible_len(text: &str) -> usize {
    text.chars().filter(|c| !c.is_whitespace()).count()
}

/// Joins the segments. Each empty token takes one of the separators around it: the one that
/// would be left without a neighbour, or the lighter one when both sides have one.
fn join_segments(segments: Vec<Segment>) -> String {
    let mut out: Vec<Segment> = Vec::new();
    let mut pending_empty = false;
    for segment in segments {
        match segment {
            Segment::Empty => pending_empty = true,
            Segment::Literal(mut text) if pending_empty => {
                pending_empty = false;
                let head = separator_head(&text);
                match out.last_mut() {
                    None => {
                        text.drain(..head);
                    }
                    Some(Segment::Literal(before)) => {
                        let tail = separator_tail(before);
                        let before_sep = &before[before.len() - tail..];
                        if tail == 0
                            || (head > 0 && visible_len(before_sep) >= visible_len(&text[..head]))
                        {
                            text.drain(..head);
                        } else {
                            before.truncate(before.len() - tail);
                        }
                    }
                    Some(_) => {}
                }
                out.push(Segment::Literal(text));
            }
            other => {
                pending_empty = false;
                out.push(other);
            }
        }
    }
    if pending_empty && let Some(Segment::Literal(text)) = out.last_mut() {
        text.truncate(text.len() - separator_tail(text));
    }
    out.into_iter()
        .map(|segment| match segment {
            Segment::Literal(text) | Segment::Value(text) => text,
            Segment::Empty => String::new(),
        })
        .collect()
}

/// Tag values are written the way a DJ reads them in a file name: whole BPM, two-digit track
/// number, four-digit year.
fn token_value(field: TagField, raw: &str) -> Option<String> {
    let raw = raw.trim();
    match field {
        TagField::Bpm => raw
            .parse::<f64>()
            .ok()
            .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
            .map(|bpm| format!("{}", bpm.round() as u32)),
        TagField::TrackNumber => leading_number(raw).map(|number| format!("{number:02}")),
        TagField::Year => {
            let digits: String = raw.chars().take_while(char::is_ascii_digit).collect();
            (digits.len() == 4).then_some(digits)
        }
        _ => Some(raw.to_owned()),
    }
}

fn leading_number(text: &str) -> Option<u32> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|number| *number > 0)
}

/// Replaces characters Windows, macOS or Linux refuse in a file name.
pub fn sanitise(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '/' | '\\' | '|' | ':' => Some('-'),
            '"' => Some('\''),
            '<' | '>' | '?' | '*' => None,
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

fn finish_stem(text: &str) -> String {
    let collapsed = sanitise(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    collapsed.trim_matches(is_separator_char).to_owned()
}

/// The full file name for `stem` with `extension`, no longer than `max_bytes`, with no trailing
/// dot or space, and never a name Windows reserves for a device.
pub fn file_name(stem: &str, extension: &str, max_bytes: usize) -> String {
    let suffix = if extension.is_empty() {
        String::new()
    } else {
        format!(".{extension}")
    };
    let budget = max_bytes.saturating_sub(suffix.len()).max(1);
    let mut stem = finish_stem(stem);
    if stem.len() > budget {
        let mut end = budget;
        while !stem.is_char_boundary(end) {
            end -= 1;
        }
        stem.truncate(end);
        stem = finish_stem(&stem);
    }
    let device = stem.split('.').next().unwrap_or_default().trim_end();
    if RESERVED_STEMS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(device))
    {
        stem.insert(0, '_');
    }
    format!("{stem}{suffix}")
}

/// What renaming one file would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenameStatus {
    Unchanged,
    Ready,
    /// Another file in the folder has the name already, or another file in the batch takes it.
    Collision,
    MissingFields,
    /// The folder's path leaves too little room for a name.
    TooLong,
    /// The file is not where the library last saw it.
    FileMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamePlan {
    pub status: RenameStatus,
    /// Set unless tags are missing or there is no room.
    pub new_name: Option<String>,
    pub missing: Vec<TagField>,
}

/// Names are compared without case, because NTFS, APFS and exFAT ignore it and a library moves
/// between them and Linux.
pub fn name_key(name: &str) -> String {
    name.to_lowercase()
}

/// True when only the case of the name changes. Such a rename needs a step through another name
/// on file systems that ignore case.
pub fn is_case_only(old: &str, new: &str) -> bool {
    old != new && name_key(old) == name_key(new)
}

fn split_extension(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, extension),
        _ => (name, ""),
    }
}

/// The name `template` gives the file now called `current_name`, in a folder whose full path is
/// `folder_units` UTF-16 units long. The extension never changes. Collisions are not known here;
/// [`NameClaims`] decides them.
pub fn plan_name<'a>(
    template: &Template,
    value: impl Fn(TagField) -> Option<&'a str>,
    current_name: &str,
    folder_units: usize,
) -> NamePlan {
    let stem = match template.render(value) {
        Rendered::Stem(stem) => stem,
        Rendered::Missing(missing) => {
            return NamePlan {
                status: RenameStatus::MissingFields,
                new_name: None,
                missing,
            };
        }
    };
    let (_, extension) = split_extension(current_name);
    let room = MAX_PATH_UNITS.saturating_sub(folder_units + 1);
    if room < extension.len() + 2 {
        return NamePlan {
            status: RenameStatus::TooLong,
            new_name: None,
            missing: Vec::new(),
        };
    }
    // A UTF-16 unit is never more than three UTF-8 bytes long, and a byte limit is stricter than
    // a unit limit, so holding the name to `room` bytes keeps it within both.
    let new_name = file_name(&stem, extension, room.min(MAX_NAME_BYTES));
    let status = if new_name == current_name {
        RenameStatus::Unchanged
    } else {
        RenameStatus::Ready
    };
    NamePlan {
        status,
        new_name: Some(new_name),
        missing: Vec::new(),
    }
}

/// The names taken in each folder a batch renames into. A name another file has, or another
/// file in the batch has claimed, is never given out again, so a rename never overwrites.
/// Names files leave behind stay taken until the batch is over.
#[derive(Debug, Default)]
pub struct NameClaims {
    folders: HashMap<String, HashSet<String>>,
}

impl NameClaims {
    pub fn knows(&self, folder: &str) -> bool {
        self.folders.contains_key(folder)
    }

    /// Records what is in `folder` on disk.
    pub fn add_folder<'a>(&mut self, folder: &str, names: impl IntoIterator<Item = &'a str>) {
        self.folders
            .entry(folder.to_owned())
            .or_default()
            .extend(names.into_iter().map(name_key));
    }

    /// Claims `new_name` for the file now called `current_name`. False when it is taken.
    pub fn claim(&mut self, folder: &str, current_name: &str, new_name: &str) -> bool {
        let key = name_key(new_name);
        if key == name_key(current_name) {
            return true;
        }
        self.folders
            .entry(folder.to_owned())
            .or_default()
            .insert(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(template: &str, tags: &[(TagField, &str)]) -> Rendered {
        let template = Template::parse(template).expect("template");
        template.render(|field| {
            tags.iter()
                .find(|(tag, _)| *tag == field)
                .map(|(_, value)| *value)
        })
    }

    fn stem(template: &str, tags: &[(TagField, &str)]) -> String {
        match render(template, tags) {
            Rendered::Stem(stem) => stem,
            Rendered::Missing(missing) => panic!("missing {missing:?}"),
        }
    }

    const ARTIST: (TagField, &str) = (TagField::Artist, "Artist");
    const TITLE: (TagField, &str) = (TagField::Title, "Title");

    #[test]
    fn fills_every_token() {
        let tags = [
            ARTIST,
            TITLE,
            (TagField::Mix, "Extended Mix"),
            (TagField::Bpm, "127.6"),
            (TagField::Key, "8A"),
            (TagField::Label, "Label"),
            (TagField::CatNo, "LBL001"),
            (TagField::Year, "2019-05-03"),
            (TagField::TrackNumber, "3/12"),
        ];
        assert_eq!(
            stem(
                "{track} {artist} - {title} ({mix}) [{bpm} {key}] {label} {catno} {year}",
                &tags
            ),
            "03 Artist - Title (Extended Mix) [128 8A] Label LBL001 2019"
        );
    }

    #[test]
    fn an_empty_mix_leaves_no_brackets() {
        assert_eq!(
            stem("{artist} - {title} ({mix})", &[ARTIST, TITLE]),
            "Artist - Title"
        );
        assert_eq!(
            stem("{artist} - {title} [{mix}] ({label})", &[ARTIST, TITLE]),
            "Artist - Title"
        );
        assert_eq!(
            stem("{artist} - {title} ({mix}, {label})", &[ARTIST, TITLE]),
            "Artist - Title"
        );
    }

    #[test]
    fn a_bracket_with_one_value_keeps_it_without_the_separator() {
        assert_eq!(
            stem(
                "{artist} - {title} ({mix}, {label})",
                &[ARTIST, TITLE, (TagField::Label, "Label")]
            ),
            "Artist - Title (Label)"
        );
        assert_eq!(
            stem(
                "{artist} - {title} ({mix}, {label})",
                &[ARTIST, TITLE, (TagField::Mix, "Dub")]
            ),
            "Artist - Title (Dub)"
        );
    }

    #[test]
    fn an_empty_token_takes_one_separator_with_it() {
        assert_eq!(
            stem("{catno} - {artist} - {title}", &[ARTIST, TITLE]),
            "Artist - Title"
        );
        assert_eq!(
            stem("{artist} - {key} - {title}", &[ARTIST, TITLE]),
            "Artist - Title"
        );
        assert_eq!(
            stem("{bpm} {key} - {artist} - {title}", &[ARTIST, TITLE]),
            "Artist - Title"
        );
        assert_eq!(
            stem("{artist} - {title} - {bpm}", &[ARTIST, TITLE]),
            "Artist - Title"
        );
        assert_eq!(
            stem("{key}{bpm} {artist} - {title}", &[ARTIST, TITLE]),
            "Artist - Title"
        );
    }

    #[test]
    fn literal_brackets_without_tokens_stay() {
        assert_eq!(
            stem("{artist} - {title} (DJ edit)", &[ARTIST, TITLE]),
            "Artist - Title (DJ edit)"
        );
    }

    #[test]
    fn a_required_tag_that_is_empty_is_missing() {
        assert_eq!(
            render("{artist} - {title}", &[TITLE]),
            Rendered::Missing(vec![TagField::Artist])
        );
        assert_eq!(
            render("{artist} - {title}", &[(TagField::Artist, "  "), TITLE]),
            Rendered::Missing(vec![TagField::Artist])
        );
        assert_eq!(render("{bpm}", &[]), Rendered::Missing(vec![TagField::Bpm]));
    }

    #[test]
    fn values_that_do_not_parse_count_as_empty() {
        assert_eq!(
            stem(
                "{title} {bpm} {year} {track}",
                &[
                    TITLE,
                    (TagField::Bpm, "fast"),
                    (TagField::Year, "05"),
                    (TagField::TrackNumber, "A1")
                ]
            ),
            "Title"
        );
    }

    #[test]
    fn forbidden_characters_are_replaced() {
        assert_eq!(
            stem(
                "{artist} - {title}",
                &[
                    (TagField::Artist, "AC/DC"),
                    (TagField::Title, "What? \"Now\": <Live> *|\\\u{7}")
                ]
            ),
            "AC-DC - What 'Now'- Live"
        );
    }

    #[test]
    fn templates_are_checked() {
        assert_eq!(Template::parse("  "), Err(TemplateError::Empty));
        assert_eq!(
            Template::parse("{artist - x"),
            Err(TemplateError::Unclosed { at: 0 })
        );
        assert_eq!(
            Template::parse("{genre}"),
            Err(TemplateError::UnknownToken {
                token: "genre".to_owned()
            })
        );
        assert_eq!(Template::parse("track"), Err(TemplateError::NoToken));
        assert!(Template::parse("{ Artist } - {TITLE}").is_ok());
        assert!(Template::parse(DEFAULT_TEMPLATE).is_ok());
    }

    #[test]
    fn trailing_dots_and_spaces_are_removed() {
        assert_eq!(file_name("Title. . ", "flac", MAX_NAME_BYTES), "Title.flac");
        assert_eq!(
            stem("{artist} - {title}", &[ARTIST, (TagField::Title, "Mr...")]),
            "Artist - Mr"
        );
        assert_eq!(file_name("...Title", "mp3", MAX_NAME_BYTES), "Title.mp3");
    }

    #[test]
    fn reserved_windows_names_get_a_prefix() {
        for reserved in ["CON", "nul", "Com1", "LPT9", "aux.live", "PRN "] {
            let name = file_name(reserved, "wav", MAX_NAME_BYTES);
            assert!(name.starts_with('_'), "{name}");
        }
        assert_eq!(file_name("Console", "wav", MAX_NAME_BYTES), "Console.wav");
        assert_eq!(file_name("COM10", "wav", MAX_NAME_BYTES), "COM10.wav");
    }

    #[test]
    fn long_names_are_cut_on_a_character_boundary() {
        let long = "é".repeat(200);
        let name = file_name(&long, "flac", MAX_NAME_BYTES);
        assert!(name.len() <= MAX_NAME_BYTES);
        assert!(name.ends_with("é.flac"));
        assert_eq!(file_name("Artist - Title", "flac", 12), "Artist.flac");
    }

    #[test]
    fn the_extension_is_kept_as_given() {
        assert_eq!(file_name("Title", "AIFF", MAX_NAME_BYTES), "Title.AIFF");
        assert_eq!(file_name("Title", "", MAX_NAME_BYTES), "Title");
    }

    #[test]
    fn a_plan_keeps_the_extension_and_says_what_changes() {
        let template = Template::parse(DEFAULT_TEMPLATE).expect("template");
        let tags = [ARTIST, TITLE];
        let value = |field| {
            tags.iter()
                .find(|(tag, _)| *tag == field)
                .map(|(_, value)| *value)
        };
        let plan = plan_name(&template, value, "01 track.FLAC", 20);
        assert_eq!(plan.status, RenameStatus::Ready);
        assert_eq!(plan.new_name.as_deref(), Some("Artist - Title.FLAC"));
        let same = plan_name(&template, value, "Artist - Title.FLAC", 20);
        assert_eq!(same.status, RenameStatus::Unchanged);
        let case = plan_name(&template, value, "artist - title.FLAC", 20);
        assert_eq!(case.status, RenameStatus::Ready);
        assert!(is_case_only("artist - title.FLAC", "Artist - Title.FLAC"));
        let missing = plan_name(&template, |_| None, "a.flac", 20);
        assert_eq!(missing.status, RenameStatus::MissingFields);
        assert_eq!(missing.missing, vec![TagField::Artist, TagField::Title]);
    }

    #[test]
    fn a_plan_stays_under_the_path_limit() {
        let template = Template::parse("{title}").expect("template");
        let long = "x".repeat(300);
        let plan = plan_name(
            &template,
            |_| Some(long.as_str()),
            "a.wav",
            MAX_PATH_UNITS - 20,
        );
        let name = plan.new_name.expect("name");
        assert_eq!(name.len(), 19);
        assert!(name.ends_with(".wav"));
        let full = plan_name(&template, |_| Some("x"), "a.wav", MAX_PATH_UNITS - 3);
        assert_eq!(full.status, RenameStatus::TooLong);
    }

    #[test]
    fn claims_block_taken_names_without_case() {
        let mut claims = NameClaims::default();
        claims.add_folder("d", ["Taken.flac", "old.flac", "other.flac"]);
        assert!(!claims.claim("d", "old.flac", "taken.FLAC"));
        assert!(claims.claim("d", "old.flac", "Old.flac"));
        assert!(claims.claim("d", "old.flac", "New.flac"));
        assert!(!claims.claim("d", "other.flac", "new.flac"));
        assert!(!claims.claim("d", "other.flac", "old.flac"));
        assert!(claims.claim("e", "a.flac", "new.flac"));
    }
}
