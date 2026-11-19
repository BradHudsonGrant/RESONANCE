use std::path::{Component, Path, PathBuf};

use crate::key::Key;

/// The audio formats the library takes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFormat {
    Mp3,
    Flac,
    Aiff,
    Wav,
    M4a,
}

impl AudioFormat {
    pub const ALL: [Self; 5] = [Self::Mp3, Self::Flac, Self::Aiff, Self::Wav, Self::M4a];

    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "mp3" => Some(Self::Mp3),
            "flac" => Some(Self::Flac),
            "aif" | "aiff" => Some(Self::Aiff),
            "wav" => Some(Self::Wav),
            "m4a" => Some(Self::M4a),
            _ => None,
        }
    }

    pub fn of(path: &Path) -> Option<Self> {
        Self::from_extension(path.extension()?.to_str()?)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Flac => "flac",
            Self::Aiff => "aiff",
            Self::Wav => "wav",
            Self::M4a => "m4a",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.name() == name)
    }
}

/// A tag a track can carry, in a file or in the library's own layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TagField {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Genre,
    Year,
    TrackNumber,
    Bpm,
    Key,
    Label,
    Comment,
    Mix,
    Subgenre,
    CatNo,
}

impl TagField {
    pub const ALL: [Self; 14] = [
        Self::Title,
        Self::Artist,
        Self::Album,
        Self::AlbumArtist,
        Self::Genre,
        Self::Year,
        Self::TrackNumber,
        Self::Bpm,
        Self::Key,
        Self::Label,
        Self::Comment,
        Self::Mix,
        Self::Subgenre,
        Self::CatNo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::AlbumArtist => "album_artist",
            Self::Genre => "genre",
            Self::Year => "year",
            Self::TrackNumber => "track_number",
            Self::Bpm => "bpm",
            Self::Key => "key",
            Self::Label => "label",
            Self::Comment => "comment",
            Self::Mix => "mix",
            Self::Subgenre => "subgenre",
            Self::CatNo => "catno",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
    }

    /// No tag format has a common frame for the mix or subgenre, so those stay in the library.
    pub fn is_written_to_files(self) -> bool {
        !matches!(self, Self::Mix | Self::Subgenre)
    }
}

/// What the file's header says about its audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFacts {
    pub duration_ms: u64,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub bit_depth: Option<u8>,
    pub bitrate_kbps: Option<u32>,
}

/// What a scan learns about one file without decoding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Relative to the source folder, with forward slashes on every platform.
    pub path: String,
    pub size_bytes: u64,
    pub modified_ns: i64,
    pub format: AudioFormat,
    /// Missing when the file could not be read as audio. It is still a file in the library.
    pub audio: Option<AudioFacts>,
    pub tags: Vec<(TagField, String)>,
}

/// Title tag when non-blank, else the file's name without its extension.
pub fn title_from_tags<'a>(path: &'a str, tags: &'a [(TagField, String)]) -> &'a str {
    let title_tag = tags
        .iter()
        .find(|(field, _)| *field == TagField::Title)
        .map(|(_, value)| value.trim())
        .filter(|title| !title.is_empty());
    match title_tag {
        Some(title) => title,
        None => {
            let name = path.rsplit('/').next().unwrap_or(path);
            name.rsplit_once('.').map_or(name, |(stem, _)| stem).trim()
        }
    }
}

/// Positive finite BPM parsed from a tag list, accepting decimal commas.
pub fn bpm_from_tags(tags: &[(TagField, String)]) -> Option<f64> {
    let raw = tags
        .iter()
        .find(|(field, _)| *field == TagField::Bpm)
        .map(|(_, value)| value.as_str())?;
    let bpm: f64 = raw.trim().replace(',', ".").parse().ok()?;
    (bpm.is_finite() && bpm > 0.0).then_some(bpm)
}

impl ScannedFile {
    pub fn tag(&self, field: TagField) -> Option<&str> {
        self.tags
            .iter()
            .find(|(tagged, _)| *tagged == field)
            .map(|(_, value)| value.as_str())
    }

    /// The title tag, or the file's name without its extension when the tag is missing or blank,
    /// so every track has something to be listed by.
    pub fn title(&self) -> &str {
        title_from_tags(&self.path, &self.tags)
    }

    /// The BPM tag as a number. Software writes it as `128`, `128.00` or, in some locales,
    /// `127,5`. Zero is how some of it writes "unknown", so it reads as no tempo.
    pub fn bpm(&self) -> Option<f64> {
        bpm_from_tags(&self.tags)
    }

    pub fn key(&self) -> Option<Key> {
        self.tag(TagField::Key)?.parse().ok()
    }

    /// The file on this machine, given the folder of the source it was scanned from.
    pub fn location(&self, source_folder: &Path) -> PathBuf {
        let mut location = source_folder.to_owned();
        location.extend(self.path.split('/'));
        location
    }
}

/// Byte offset immediately after a valid 10-byte ID3v2 header, including any 10-byte footer.
///
/// Some taggers prepend an ID3v2 block to an MP4 container; callers check whether the eight bytes
/// at this offset begin an `ftyp` box ([`is_mp4_ftyp_box`]) before stepping over the tag.
pub fn id3v2_tag_end(header: &[u8; 10]) -> Option<u64> {
    if !header.starts_with(b"ID3") || header[6..].iter().any(|byte| byte & 0x80 != 0) {
        return None;
    }
    let size = header[6..]
        .iter()
        .fold(0u64, |acc, &byte| (acc << 7) | u64::from(byte));
    let footer = if header[5] & 0x10 == 0 { 0 } else { 10 };
    Some(10 + size + footer)
}

/// Whether the eight-byte box header starts an ISO base media (`ftyp`) container.
pub fn is_mp4_ftyp_box(box_header: &[u8; 8]) -> bool {
    &box_header[4..] == b"ftyp"
}

/// Where the old app parked files it had replaced, converted or rejected. They sit inside music
/// folders, and scanning them would bring every one back as a track. They seed the editable
/// ignored-folder list in a fresh library.
pub const DEFAULT_IGNORED_FOLDERS: [&str; 4] = [
    "_replaced",
    "_wrong_match",
    "_trash",
    "_converted_originals",
];

/// A folder containing this marker is excluded as a source and while walking a parent.
/// An explicit source below that folder can still be scanned.
pub const SCAN_IGNORE_MARKER: &str = ".resonance-ignore";

/// Whether a scan stays out of a folder with this name under the default ignore list.
pub fn is_ignored_folder(name: &str) -> bool {
    is_ignored_folder_with(name, &DEFAULT_IGNORED_FOLDERS)
}

/// Whether a scan stays out of a folder with this name, given the library's editable list of
/// ignored folder names. Hidden dot-folders are always skipped.
pub fn is_ignored_folder_with<S: AsRef<str>>(name: &str, ignored_folders: &[S]) -> bool {
    name.starts_with('.')
        || ignored_folders
            .iter()
            .any(|ignored| name.eq_ignore_ascii_case(ignored.as_ref()))
}

/// Cleans user-supplied folder names for the editable ignore list. Blank entries, dot-folders and
/// entries containing path separators are dropped, and case-insensitive duplicates keep their
/// first spelling.
pub fn normalise_ignored_folders<S: AsRef<str>>(names: &[S]) -> Vec<String> {
    let mut cleaned: Vec<String> = Vec::new();
    for raw in names {
        let trimmed = raw.as_ref().trim();
        if trimmed.is_empty()
            || trimmed.starts_with('.')
            || trimmed.contains('/')
            || trimmed.contains('\\')
        {
            continue;
        }
        if !cleaned
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(trimmed))
        {
            cleaned.push(trimmed.to_owned());
        }
    }
    cleaned
}

/// Normalises a relative subfolder path to forward slashes without stripping trailing spaces from
/// folder names, because real music folders on Windows can end with a space.
pub fn normalise_subfolder_path(relative: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in relative.split(['/', '\\']) {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." {
            return None;
        }
        parts.push(part);
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Normalises, sorts and deduplicates a source's excluded relative subfolder paths.
pub fn normalise_excluded_subfolders<S: AsRef<str>>(paths: &[S]) -> Vec<String> {
    let mut cleaned: Vec<String> = paths
        .iter()
        .filter_map(|path| normalise_subfolder_path(path.as_ref()))
        .collect();
    cleaned.sort();
    cleaned.dedup();
    cleaned
}

/// Whether `relative_folder` (or one of its ancestors inside the source) has been switched off.
pub fn is_subfolder_excluded<S: AsRef<str>>(
    relative_folder: &str,
    excluded_subfolders: &[S],
) -> bool {
    let Some(folder) = normalise_subfolder_path(relative_folder) else {
        return false;
    };
    excluded_subfolders.iter().any(|entry| {
        let Some(excluded) = normalise_subfolder_path(entry.as_ref()) else {
            return false;
        };
        folder == excluded
            || folder
                .strip_prefix(&excluded)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Whether an asset's relative path inside its source belongs in the library under that source's
/// current subfolder settings and the library's ignored folder names.
pub fn is_asset_path_included<S1: AsRef<str>, S2: AsRef<str>>(
    asset_path: &str,
    include_subfolders: bool,
    excluded_subfolders: &[S1],
    ignored_folders: &[S2],
) -> bool {
    let Some((parent, _file_name)) = asset_path.rsplit_once('/') else {
        return true;
    };
    if !include_subfolders {
        return false;
    }
    if parent
        .split('/')
        .any(|segment| !segment.is_empty() && is_ignored_folder_with(segment, ignored_folders))
    {
        return false;
    }
    !is_subfolder_excluded(parent, excluded_subfolders)
}

/// The folder directly under `source` that holds `inside`.
///
/// A scan skips this folder for the program's own files, so that a copy of the app kept inside
/// the music folder never has its test audio or its data scanned in as tracks.
pub fn folder_under_source_holding(source: &Path, inside: &Path) -> Option<PathBuf> {
    let below = inside.strip_prefix(source).ok()?;
    match below.components().next()? {
        Component::Normal(first) => Some(source.join(first)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_formats_are_known_by_extension_in_any_case() {
        assert_eq!(
            AudioFormat::of(Path::new("a/one.mp3")),
            Some(AudioFormat::Mp3)
        );
        assert_eq!(
            AudioFormat::of(Path::new("two.FLAC")),
            Some(AudioFormat::Flac)
        );
        assert_eq!(
            AudioFormat::of(Path::new("three.aif")),
            Some(AudioFormat::Aiff)
        );
        assert_eq!(
            AudioFormat::of(Path::new("three.Aiff")),
            Some(AudioFormat::Aiff)
        );
        assert_eq!(
            AudioFormat::of(Path::new("four.wav")),
            Some(AudioFormat::Wav)
        );
        assert_eq!(
            AudioFormat::of(Path::new("five.m4a")),
            Some(AudioFormat::M4a)
        );
    }

    #[test]
    fn other_files_are_not_audio() {
        assert_eq!(AudioFormat::of(Path::new("cover.jpg")), None);
        assert_eq!(AudioFormat::of(Path::new("set.ogg")), None);
        assert_eq!(AudioFormat::of(Path::new("mp3")), None);
    }

    #[test]
    fn format_and_tag_names_read_back_as_themselves() {
        for format in AudioFormat::ALL {
            assert_eq!(AudioFormat::from_name(format.name()), Some(format));
        }
        for field in TagField::ALL {
            assert_eq!(TagField::from_name(field.name()), Some(field));
        }
    }

    fn tagged(path: &str, tags: &[(TagField, &str)]) -> ScannedFile {
        ScannedFile {
            path: path.to_owned(),
            size_bytes: 1,
            modified_ns: 0,
            format: AudioFormat::Mp3,
            audio: None,
            tags: tags
                .iter()
                .map(|(field, value)| (*field, (*value).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn the_title_tag_names_the_track() {
        let file = tagged("House/a.mp3", &[(TagField::Title, " Blue Hour ")]);

        assert_eq!(file.title(), "Blue Hour");
    }

    #[test]
    fn a_file_without_a_title_is_named_by_its_file_name() {
        assert_eq!(
            tagged("House/Ada Marsh - Blue Hour.mp3", &[]).title(),
            "Ada Marsh - Blue Hour"
        );
        assert_eq!(
            tagged("v1.2 edit.flac", &[(TagField::Title, "  ")]).title(),
            "v1.2 edit"
        );
        assert_eq!(tagged("no extension", &[]).title(), "no extension");
    }

    #[test]
    fn bpm_tags_read_in_the_forms_software_writes() {
        for (text, bpm) in [
            ("128", 128.0),
            ("128.00", 128.0),
            ("127,5", 127.5),
            (" 90 ", 90.0),
        ] {
            assert_eq!(
                tagged("a.mp3", &[(TagField::Bpm, text)]).bpm(),
                Some(bpm),
                "{text}"
            );
        }
    }

    #[test]
    fn a_zero_or_unreadable_bpm_tag_is_no_tempo() {
        for text in ["0", "", "fast", "NaN", "-120", "inf"] {
            assert_eq!(
                tagged("a.mp3", &[(TagField::Bpm, text)]).bpm(),
                None,
                "{text}"
            );
        }
        assert_eq!(tagged("a.mp3", &[]).bpm(), None);
    }

    #[test]
    fn a_key_tag_reads_as_a_key_and_anything_else_as_none() {
        let key = tagged("a.mp3", &[(TagField::Key, "Am")]).key();

        assert_eq!(key.map(Key::camelot).as_deref(), Some("8A"));
        assert_eq!(tagged("a.mp3", &[(TagField::Key, "Am/C")]).key(), None);
    }

    #[test]
    fn a_file_is_found_under_its_source_folder_with_the_platform_separator() {
        let file = tagged("House/2024/one.mp3", &[]);

        assert_eq!(
            file.location(Path::new("music")),
            Path::new("music")
                .join("House")
                .join("2024")
                .join("one.mp3")
        );
    }

    #[test]
    fn hidden_folders_and_the_old_quarantine_folders_are_ignored() {
        for name in [
            ".git",
            ".Trash-1000",
            "_replaced",
            "_wrong_match",
            "_trash",
            "_converted_originals",
            "_TRASH",
        ] {
            assert!(is_ignored_folder(name), "{name} should be ignored");
        }
    }

    #[test]
    fn ordinary_folders_are_not_ignored() {
        for name in ["House", "_new", "replaced", "Garage n Grime", "trash"] {
            assert!(!is_ignored_folder(name), "{name} should be scanned");
        }
    }

    #[test]
    fn the_folder_holding_the_program_is_the_one_directly_under_the_source() {
        let source = Path::new("music");
        let program = Path::new("music/RESONANCE/target/debug");

        assert_eq!(
            folder_under_source_holding(source, program),
            Some(PathBuf::from("music/RESONANCE"))
        );
    }

    #[test]
    fn a_program_outside_the_source_or_at_its_top_gives_nothing_to_skip() {
        let source = Path::new("music");

        assert_eq!(
            folder_under_source_holding(source, Path::new("apps/resonance")),
            None
        );
        assert_eq!(
            folder_under_source_holding(source, Path::new("music")),
            None
        );
    }

    #[test]
    fn editable_ignored_folders_clean_duplicates_and_keep_dot_folders_always_ignored() {
        let cleaned = normalise_ignored_folders(&[
            "  Samples  ",
            "samples",
            "",
            ".git",
            "../up",
            "sub/path",
            r"sub\path",
            "_trash",
        ]);

        assert_eq!(cleaned, vec!["Samples", "_trash"]);
        assert!(is_ignored_folder_with("SAMPLES", &cleaned));
        assert!(is_ignored_folder_with(".hidden", &cleaned));
        assert!(!is_ignored_folder_with("_replaced", &cleaned));
    }

    #[test]
    fn subfolder_paths_keep_trailing_spaces_and_reject_traversal() {
        assert_eq!(
            normalise_subfolder_path(r"\Garage \140/"),
            Some("Garage /140".to_owned())
        );
        assert_eq!(normalise_subfolder_path(""), None);
        assert_eq!(normalise_subfolder_path("House/../Samples"), None);
        assert_eq!(normalise_subfolder_path("."), None);
    }

    #[test]
    fn excluding_a_subfolder_excludes_its_children_but_not_lookalike_siblings() {
        let excluded = normalise_excluded_subfolders(&["House", r"Garage \Old"]);

        assert!(is_subfolder_excluded("House", &excluded));
        assert!(is_subfolder_excluded("House/Deep/2025", &excluded));
        assert!(is_subfolder_excluded("Garage /Old/Clips", &excluded));
        assert!(!is_subfolder_excluded("Houseparty", &excluded));
        assert!(!is_subfolder_excluded("Garage/Old", &excluded));
    }

    #[test]
    fn asset_inclusion_follows_subfolder_toggle_exclusions_and_ignore_list() {
        let excluded = ["Samples"];
        let ignored = ["_trash", "Podcasts"];

        assert!(is_asset_path_included(
            "root.flac",
            false,
            &excluded,
            &ignored
        ));
        assert!(!is_asset_path_included(
            "House/deep.flac",
            false,
            &excluded,
            &ignored
        ));
        assert!(is_asset_path_included(
            "House/deep.flac",
            true,
            &excluded,
            &ignored
        ));
        assert!(!is_asset_path_included(
            "Samples/Loops/kick.wav",
            true,
            &excluded,
            &ignored
        ));
        assert!(!is_asset_path_included(
            "House/podcasts/ep1.mp3",
            true,
            &excluded,
            &ignored
        ));
    }

    #[test]
    fn id3v2_header_end_and_mp4_ftyp_match_syncsafe_sizes_and_footers() {
        assert_eq!(id3v2_tag_end(b"ID3\x03\x00\x00\x00\x00\x00\x04"), Some(14));
        assert_eq!(id3v2_tag_end(b"ID3\x04\x00\x10\x00\x00\x01\x00"), Some(148));
        assert_eq!(id3v2_tag_end(b"ID3\x03\x00\x00\x00\x00\x80\x00"), None);
        assert_eq!(id3v2_tag_end(b"fLaC\x00\x00\x00\x22\x00\x00"), None);
        assert!(is_mp4_ftyp_box(b"\x00\x00\x00\x14ftyp"));
        assert!(!is_mp4_ftyp_box(b"RIFF1234"));
    }
}
