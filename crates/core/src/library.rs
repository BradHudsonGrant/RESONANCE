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

/// A tag the library reads from a file.
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
}

impl TagField {
    pub const ALL: [Self; 11] = [
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
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
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
        match self.tag(TagField::Title).map(str::trim) {
            Some(title) if !title.is_empty() => title,
            _ => {
                let name = self.path.rsplit('/').next().unwrap_or(&self.path);
                name.rsplit_once('.').map_or(name, |(stem, _)| stem).trim()
            }
        }
    }

    /// The BPM tag as a number. Software writes it as `128`, `128.00` or, in some locales,
    /// `127,5`. Zero is how some of it writes "unknown", so it reads as no tempo.
    pub fn bpm(&self) -> Option<f64> {
        let bpm: f64 = self
            .tag(TagField::Bpm)?
            .trim()
            .replace(',', ".")
            .parse()
            .ok()?;
        (bpm.is_finite() && bpm > 0.0).then_some(bpm)
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

/// Where the old app parked files it had replaced, converted or rejected. They sit inside music
/// folders, and scanning them would bring every one back as a track.
const QUARANTINE_FOLDERS: [&str; 4] = [
    "_replaced",
    "_wrong_match",
    "_trash",
    "_converted_originals",
];

/// A folder containing this marker is excluded as a source and while walking a parent.
/// An explicit source below that folder can still be scanned.
pub const SCAN_IGNORE_MARKER: &str = ".resonance-ignore";

/// Whether a scan stays out of a folder with this name, at any depth.
pub fn is_ignored_folder(name: &str) -> bool {
    name.starts_with('.')
        || QUARANTINE_FOLDERS
            .iter()
            .any(|quarantine| name.eq_ignore_ascii_case(quarantine))
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
}
