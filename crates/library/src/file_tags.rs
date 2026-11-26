use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use lofty::config::{ParseOptions, ParsingMode};
use lofty::error::FileParseError;
use lofty::file::TaggedFile;
use lofty::picture::PictureType;
use lofty::prelude::{AudioFile, ItemKey, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Tag;
use resonance_core::identify::extract_isrc;
use resonance_core::library::{AudioFacts, TagField, id3v2_tag_end, is_mp4_ftyp_box};

#[derive(Debug)]
pub enum FileReadError {
    Open(io::Error),
    NotAudio(FileParseError),
}

impl fmt::Display for FileReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(source) => write!(f, "The file could not be opened: {source}"),
            Self::NotAudio(source) => write!(f, "The file could not be read as audio: {source}"),
        }
    }
}

impl std::error::Error for FileReadError {}

pub struct FileContents {
    pub audio: AudioFacts,
    pub tags: Vec<(TagField, String)>,
    /// Embedded ISO 3901 ISRC frame, if any.
    pub isrc: Option<String>,
    /// The bytes of the picture the file shows as its cover.
    pub front_cover: Option<Vec<u8>>,
}

/// The keys a field can sit under, most specific first.
pub(crate) fn keys(field: TagField) -> &'static [ItemKey] {
    match field {
        TagField::Title => &[ItemKey::TrackTitle],
        TagField::Artist => &[ItemKey::TrackArtist],
        TagField::Album => &[ItemKey::AlbumTitle],
        TagField::AlbumArtist => &[ItemKey::AlbumArtist],
        TagField::Genre => &[ItemKey::Genre],
        TagField::Year => &[ItemKey::Year, ItemKey::RecordingDate],
        TagField::TrackNumber => &[ItemKey::TrackNumber],
        TagField::Bpm => &[ItemKey::Bpm, ItemKey::IntegerBpm],
        TagField::Key => &[ItemKey::InitialKey],
        TagField::Label => &[ItemKey::Label, ItemKey::Publisher],
        TagField::Comment => &[ItemKey::Comment],
        TagField::CatNo => &[ItemKey::CatalogNumber],
        TagField::Composer => &[ItemKey::Composer],
        TagField::Remixer => &[ItemKey::Remixer],
        // No tag format has a common frame for these, so they live only in the library.
        TagField::Mix | TagField::Subgenre => &[],
    }
}

/// A date tag holds anything from "2019" to a full timestamp. The year is its first four digits.
fn year(date: &str) -> Option<&str> {
    let digits = date.get(..4)?;
    digits
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then_some(digits)
}

fn value(tags: &[&Tag], field: TagField) -> Option<String> {
    tags.iter()
        .flat_map(|tag| keys(field).iter().filter_map(|key| tag.get_string(*key)))
        .map(|text| text.trim_matches(|c: char| c.is_whitespace() || c == '\0'))
        .filter_map(|text| match field {
            TagField::Year => year(text),
            _ => Some(text),
        })
        .find(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The front cover, else the first picture of any kind: many files carry their only picture
/// typed as "other".
fn front_cover(tags: &[&Tag]) -> Option<Vec<u8>> {
    let pictures = || tags.iter().flat_map(|tag| tag.pictures());
    pictures()
        .find(|picture| picture.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures().next())
        .map(|picture| picture.data().to_vec())
}

/// A read-only file handle seen from `start` onwards, as if any stray bytes before `start` were
/// not there.
pub(crate) struct OffsetFile {
    inner: BufReader<File>,
    start: u64,
    total_len: u64,
}

impl Read for OffsetFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl Seek for OffsetFile {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let position = match to {
            SeekFrom::Start(offset) => self
                .inner
                .seek(SeekFrom::Start(self.start.saturating_add(offset)))?,
            relative => self.inner.seek(relative)?,
        };
        if position < self.start {
            let _ = self.inner.seek(SeekFrom::Start(self.start));
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a seek to before the start",
            ));
        }
        Ok(position - self.start)
    }
}

impl symphonia::core::io::MediaSource for OffsetFile {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.total_len.saturating_sub(self.start))
    }
}

fn start_of_audio_file(file: &mut File) -> io::Result<u64> {
    let mut header = [0; 10];
    let mut next = [0; 8];
    if file.read_exact(&mut header).is_ok()
        && let Some(after) = id3v2_tag_end(&header)
    {
        file.seek(SeekFrom::Start(after))?;
        if file.read_exact(&mut next).is_ok() && is_mp4_ftyp_box(&next) {
            return Ok(after);
        }
    }
    Ok(0)
}

/// Whether an ID3v2 block sits in front of an MP4 container.
pub(crate) fn has_leading_id3v2(path: &Path) -> io::Result<bool> {
    Ok(start_of_audio_file(&mut File::open(path)?)? > 0)
}

/// How many bytes of ID3v2 sit in front of an MP4 container. `0` for any other file.
pub(crate) fn audio_start(path: &Path) -> io::Result<u64> {
    start_of_audio_file(&mut File::open(path)?)
}

pub(crate) fn open_audio_reader(path: &Path) -> io::Result<OffsetFile> {
    let mut file = File::open(path)?;
    let total_len = file.metadata()?.len();
    let start = start_of_audio_file(&mut file)?;
    file.seek(SeekFrom::Start(start))?;
    Ok(OffsetFile {
        inner: BufReader::new(file),
        start,
        total_len,
    })
}

/// Reads tags and file facts through a handle opened for reading only, so nothing here can
/// change the file.
pub fn read(path: &Path) -> Result<FileContents, FileReadError> {
    let probe = |options: ParseOptions| -> Result<TaggedFile, FileReadError> {
        let audio_file = open_audio_reader(path).map_err(FileReadError::Open)?;
        Probe::new(audio_file)
            .options(options)
            .guess_file_type()
            .map_err(FileReadError::Open)?
            .read()
            .map_err(FileReadError::NotAudio)
    };
    // Rips and re-encodes often carry something the default reader rejects, such as padding
    // after the first MPEG frame, while the audio and tags are sound. Those files are read again
    // leniently rather than left out of the library with no tags to search with.
    let tagged = match probe(ParseOptions::new()) {
        Ok(tagged) => tagged,
        Err(FileReadError::NotAudio(first)) => {
            probe(ParseOptions::new().parsing_mode(ParsingMode::Relaxed))
                .map_err(|_| FileReadError::NotAudio(first))?
        }
        Err(error) => return Err(error),
    };

    let properties = tagged.properties();
    let audio = AudioFacts {
        duration_ms: u64::try_from(properties.duration().as_millis()).unwrap_or(u64::MAX),
        sample_rate: properties.sample_rate(),
        channels: properties.channels(),
        bit_depth: properties.bit_depth(),
        bitrate_kbps: properties.audio_bitrate(),
    };

    // The format's own tag first, then whatever else the file carries.
    let mut in_file: Vec<&Tag> = tagged.primary_tag().into_iter().collect();
    in_file.extend(
        tagged
            .tags()
            .iter()
            .filter(|tag| Some(tag.tag_type()) != tagged.primary_tag().map(Tag::tag_type)),
    );
    let tags = TagField::ALL
        .into_iter()
        .filter_map(|field| Some((field, value(&in_file, field)?)))
        .collect();

    let isrc = in_file
        .iter()
        .filter_map(|tag| tag.get_string(ItemKey::Isrc))
        .find_map(extract_isrc);

    Ok(FileContents {
        audio,
        tags,
        isrc,
        front_cover: front_cover(&in_file),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_year_is_the_first_four_digits_of_a_date() {
        assert_eq!(year("2019"), Some("2019"));
        assert_eq!(year("2019-05-03"), Some("2019"));
        assert_eq!(year("2019-05-03T10:00:00"), Some("2019"));
    }

    #[test]
    fn text_that_does_not_start_with_a_year_gives_none() {
        assert_eq!(year("May 2019"), None);
        assert_eq!(year("19"), None);
        assert_eq!(year(""), None);
    }
}
