use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use lofty::error::FileParseError;
use lofty::prelude::{AudioFile, ItemKey, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Tag;
use resonance_core::library::{AudioFacts, TagField};

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
}

/// The keys a field can sit under, most specific first.
fn keys(field: TagField) -> &'static [ItemKey] {
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

/// A file seen from `start` onwards, as if the bytes before it were not there.
struct FromOffset<R> {
    inner: R,
    start: u64,
}

impl<R: Read> Read for FromOffset<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl<R: Seek> Seek for FromOffset<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let position = match to {
            SeekFrom::Start(offset) => self.inner.seek(SeekFrom::Start(self.start + offset))?,
            relative => self.inner.seek(relative)?,
        };
        position.checked_sub(self.start).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "a seek to before the start")
        })
    }
}

/// Where the audio file proper starts.
///
/// Some taggers put an ID3v2 tag in front of an MP4 file. The MP4 behind it is whole, but every
/// reader expects it at byte zero, so the tag is stepped over. Anything else starts at zero.
fn start_of_audio_file(file: &mut File) -> io::Result<u64> {
    let mut header = [0; 10];
    let mut next = [0; 8];
    let tagged = file.read_exact(&mut header).is_ok()
        && header.starts_with(b"ID3")
        && header[6..].iter().all(|byte| byte & 0x80 == 0);
    if tagged {
        let size = header[6..]
            .iter()
            .fold(0u64, |size, &byte| (size << 7) | u64::from(byte));
        let footer = if header[5] & 0x10 == 0 { 0 } else { 10 };
        let after = 10 + size + footer;
        file.seek(SeekFrom::Start(after))?;
        if file.read_exact(&mut next).is_ok() && &next[4..] == b"ftyp" {
            return Ok(after);
        }
    }
    Ok(0)
}

/// Reads tags and file facts through a handle opened for reading only, so nothing here can
/// change the file.
pub fn read(path: &Path) -> Result<FileContents, FileReadError> {
    let mut file = File::open(path).map_err(FileReadError::Open)?;
    let start = start_of_audio_file(&mut file).map_err(FileReadError::Open)?;
    let mut audio_file = FromOffset {
        inner: BufReader::new(file),
        start,
    };
    audio_file
        .seek(SeekFrom::Start(0))
        .map_err(FileReadError::Open)?;
    let tagged = Probe::new(audio_file)
        .guess_file_type()
        .map_err(FileReadError::Open)?
        .read()
        .map_err(FileReadError::NotAudio)?;

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

    Ok(FileContents { audio, tags })
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
