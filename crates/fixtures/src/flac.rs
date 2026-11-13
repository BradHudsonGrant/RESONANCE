use std::fs;
use std::io;
use std::path::Path;

use crate::audio_file::pcm16;
use crate::signal::Signal;

const BLOCK_FRAMES: usize = 4096;
const BITS: u64 = 16;

fn crc8(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |crc, &byte| {
        (0..8).fold(crc ^ byte, |crc: u8, _| {
            if crc & 0x80 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x07
            }
        })
    })
}

fn crc16(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0, |crc, &byte| {
        (0..8).fold(crc ^ (u16::from(byte) << 8), |crc: u16, _| {
            if crc & 0x8000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x8005
            }
        })
    })
}

/// FLAC numbers its frames the way UTF-8 encodes code points.
fn frame_number(number: u32) -> Vec<u8> {
    if number < 0x80 {
        return vec![number as u8];
    }
    let following: u32 = match number {
        0x80..0x800 => 1,
        0x800..0x1_0000 => 2,
        0x1_0000..0x20_0000 => 3,
        0x20_0000..0x400_0000 => 4,
        _ => 5,
    };
    let mut bytes = vec![(0xFFu8 << (7 - following)) | (number >> (6 * following)) as u8];
    bytes.extend(
        (0..following)
            .rev()
            .map(|index| 0x80 | ((number >> (6 * index)) & 0x3F) as u8),
    );
    bytes
}

fn too_long() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "the signal is too long for this file format",
    )
}

impl Signal {
    /// Writes 16-bit FLAC with every sample stored as it is. Nothing is compressed, which keeps
    /// the writer small, and a decoder still returns exactly the samples that went in.
    pub fn write_flac(&self, path: &Path) -> io::Result<()> {
        let channels = usize::from(self.channels);
        let total_frames = self.frames() as u64;
        if total_frames >= 1 << 36 || !(1..=8).contains(&channels) {
            return Err(too_long());
        }

        let mut file = Vec::with_capacity(self.samples.len() * 2 + 4096);
        file.extend_from_slice(b"fLaC");
        // One metadata block, the last: stream info, 34 bytes.
        file.extend_from_slice(&[0x80, 0, 0, 34]);
        file.extend_from_slice(&(BLOCK_FRAMES as u16).to_be_bytes());
        file.extend_from_slice(&(BLOCK_FRAMES as u16).to_be_bytes());
        file.extend_from_slice(&[0; 6]);
        let packed = (u64::from(self.sample_rate) << 44)
            | ((channels as u64 - 1) << 41)
            | ((BITS - 1) << 36)
            | total_frames;
        file.extend_from_slice(&packed.to_be_bytes());
        file.extend_from_slice(&[0; 16]);

        for (number, block) in self.samples.chunks(BLOCK_FRAMES * channels).enumerate() {
            let number = u32::try_from(number).map_err(|_| too_long())?;
            let frames = block.len() / channels;
            let mut frame = vec![0xFF, 0xF8, 0x70, ((channels as u8 - 1) << 4) | 0x08];
            frame.extend(frame_number(number));
            frame.extend_from_slice(&(frames as u16 - 1).to_be_bytes());
            frame.push(crc8(&frame));
            for channel in 0..channels {
                frame.push(0x02);
                for sample in block.iter().skip(channel).step_by(channels) {
                    frame.extend_from_slice(&pcm16(*sample).to_be_bytes());
                }
            }
            frame.extend_from_slice(&crc16(&frame).to_be_bytes());
            file.extend_from_slice(&frame);
        }
        fs::write(path, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksums_match_their_published_check_values() {
        assert_eq!(crc8(b"123456789"), 0xF4);
        assert_eq!(crc16(b"123456789"), 0xFEE8);
    }

    #[test]
    fn frame_numbers_are_coded_like_utf8() {
        assert_eq!(frame_number(0), [0x00]);
        assert_eq!(frame_number(0x7F), [0x7F]);
        assert_eq!(frame_number(0x80), [0xC2, 0x80]);
        assert_eq!(frame_number(0x7FF), [0xDF, 0xBF]);
        assert_eq!(frame_number(0x800), [0xE0, 0xA0, 0x80]);
        assert_eq!(frame_number(0x1_0000), [0xF0, 0x90, 0x80, 0x80]);
    }
}
