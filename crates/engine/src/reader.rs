//! Read-only audio stream opening that steps over stray ID3v2 headers ahead of MP4 containers.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use resonance_core::library::{id3v2_tag_end, is_mp4_ftyp_box};

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

/// Opens `path` read-only, skipping any leading ID3v2 tag wrapped around an MP4 container.
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Seek, SeekFrom};
    use std::path::PathBuf;

    use super::*;

    fn test_file(name: &str) -> PathBuf {
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/engine-reader");
        fs::create_dir_all(&folder).unwrap();
        folder.join(name)
    }

    #[test]
    fn skips_leading_id3v2_tag_before_mp4_ftyp() {
        let path = test_file("id3-prefixed.m4a");
        // Syncsafe size 4 (payload = b"ABCD"), followed by ftyp box header.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"ID3\x03\x00\x00\x00\x00\x00\x04ABCD");
        bytes.extend_from_slice(b"\x00\x00\x00\x14ftypM4A ");
        fs::write(&path, &bytes).unwrap();

        let mut reader = open_audio_reader(&path).unwrap();
        let mut head = [0u8; 8];
        reader.read_exact(&mut head).unwrap();
        assert_eq!(&head[4..], b"ftyp");
        assert_eq!(reader.seek(SeekFrom::Start(0)).unwrap(), 0);
        assert_eq!(reader.seek(SeekFrom::Current(4)).unwrap(), 4);
    }

    #[test]
    fn keeps_offset_zero_for_normal_files() {
        let path = test_file("normal.bin");
        fs::write(&path, b"RIFF1234WAVE").unwrap();

        let mut reader = open_audio_reader(&path).unwrap();
        let mut head = [0u8; 4];
        reader.read_exact(&mut head).unwrap();
        assert_eq!(&head, b"RIFF");
    }
}
