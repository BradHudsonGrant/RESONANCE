use std::path::{Component, Path, PathBuf};

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

/// Where the old app parked files it had replaced, converted or rejected. They sit inside music
/// folders, and scanning them would bring every one back as a track.
const QUARANTINE_FOLDERS: [&str; 4] = [
    "_replaced",
    "_wrong_match",
    "_trash",
    "_converted_originals",
];

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
