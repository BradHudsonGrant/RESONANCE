//! Silent MP3 and M4A files of a known length, built from frames that are constants.
//!
//! Encoding real audio in these formats would need an encoder. Tests that only read tags and
//! file facts do not need one: a silent frame is a few fixed bytes, and a file of them is as
//! real to a tag reader or a decoder as any other.

use std::fs;
use std::io;
use std::path::Path;

pub const SAMPLE_RATE: u32 = 44_100;

const MP3_FRAME_SAMPLES: f64 = 1152.0;
const MP3_FRAME_BYTES: usize = 417;
/// MPEG-1 Layer III, 128 kbit/s, 44.1 kHz, stereo, no padding and no checksum.
const MP3_FRAME_HEADER: [u8; 4] = [0xFF, 0xFB, 0x90, 0x00];

const AAC_FRAME_SAMPLES: u32 = 1024;
/// One frame of stereo AAC-LC that decodes to silence.
const AAC_FRAME: [u8; 9] = [0x21, 0x00, 0x49, 0x90, 0x02, 0x19, 0x00, 0x23, 0x80];

fn frames_for(seconds: f64, frame_samples: f64) -> usize {
    (seconds * f64::from(SAMPLE_RATE) / frame_samples).ceil() as usize
}

/// Writes a stereo MP3 of silence. Every frame is a header followed by zeros, which says each
/// granule holds no data.
pub fn write_silent_mp3(path: &Path, seconds: f64) -> io::Result<()> {
    let mut frame = MP3_FRAME_HEADER.to_vec();
    frame.resize(MP3_FRAME_BYTES, 0);
    fs::write(path, frame.repeat(frames_for(seconds, MP3_FRAME_SAMPLES)))
}

fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(body.len() + 8);
    bytes.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(body);
    bytes
}

fn full_box(kind: &[u8; 4], flags: u32, body: &[u8]) -> Vec<u8> {
    let mut with_flags = flags.to_be_bytes().to_vec();
    with_flags.extend_from_slice(body);
    boxed(kind, &with_flags)
}

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect()
}

const UNITY_MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

fn sample_description() -> Vec<u8> {
    // AAC-LC, 44.1 kHz, two channels.
    let audio_specific_config = [0x12, 0x10];
    let bitrate = (AAC_FRAME.len() as u32 * 8 * SAMPLE_RATE).div_ceil(AAC_FRAME_SAMPLES);

    let mut decoder_config = vec![0x40, 0x15, 0, 0, 0];
    decoder_config.extend(words(&[bitrate, bitrate]));
    decoder_config.extend([0x05, audio_specific_config.len() as u8]);
    decoder_config.extend(audio_specific_config);

    let mut stream = vec![0, 0, 0];
    stream.extend([0x04, decoder_config.len() as u8]);
    stream.extend(decoder_config);
    stream.extend([0x06, 0x01, 0x02]);

    let mut descriptor = vec![0x03, stream.len() as u8];
    descriptor.extend(stream);

    let mut entry = vec![0, 0, 0, 0, 0, 0, 0, 1];
    entry.extend(words(&[0, 0]));
    entry.extend([0, 2, 0, 16, 0, 0, 0, 0]);
    entry.extend(words(&[SAMPLE_RATE << 16]));
    entry.extend(full_box(b"esds", 0, &descriptor));

    let mut body = words(&[1]);
    body.extend(boxed(b"mp4a", &entry));
    full_box(b"stsd", 0, &body)
}

fn movie(frames: u32, data_offset: u32) -> Vec<u8> {
    let duration = frames * AAC_FRAME_SAMPLES;

    let mut header = words(&[0, 0, SAMPLE_RATE, duration, 0x0001_0000]);
    header.extend([0x01, 0x00, 0, 0]);
    header.extend(words(&[0, 0]));
    header.extend(words(&UNITY_MATRIX));
    header.extend(words(&[0, 0, 0, 0, 0, 0, 2]));

    let mut track_header = words(&[0, 0, 1, 0, duration, 0, 0]);
    track_header.extend([0, 0, 0, 0, 0x01, 0x00, 0, 0]);
    track_header.extend(words(&UNITY_MATRIX));
    track_header.extend(words(&[0, 0]));

    let mut media_header = words(&[0, 0, SAMPLE_RATE, duration]);
    // The language code for "undetermined".
    media_header.extend([0x55, 0xC4, 0, 0]);

    let mut handler = words(&[0]);
    handler.extend_from_slice(b"soun");
    handler.extend(words(&[0, 0, 0]));
    handler.extend_from_slice(b"SoundHandler\0");

    let mut data_reference = words(&[1]);
    data_reference.extend(full_box(b"url ", 1, &[]));

    let mut sample_table = sample_description();
    sample_table.extend(full_box(
        b"stts",
        0,
        &words(&[1, frames, AAC_FRAME_SAMPLES]),
    ));
    sample_table.extend(full_box(b"stsc", 0, &words(&[1, 1, frames, 1])));
    sample_table.extend(full_box(
        b"stsz",
        0,
        &words(&[AAC_FRAME.len() as u32, frames]),
    ));
    sample_table.extend(full_box(b"stco", 0, &words(&[1, data_offset])));

    let mut media_information = full_box(b"smhd", 0, &[0; 4]);
    media_information.extend(boxed(b"dinf", &full_box(b"dref", 0, &data_reference)));
    media_information.extend(boxed(b"stbl", &sample_table));

    let mut media = full_box(b"mdhd", 0, &media_header);
    media.extend(full_box(b"hdlr", 0, &handler));
    media.extend(boxed(b"minf", &media_information));

    let mut track = full_box(b"tkhd", 7, &track_header);
    track.extend(boxed(b"mdia", &media));

    let mut movie = full_box(b"mvhd", 0, &header);
    movie.extend(boxed(b"trak", &track));
    boxed(b"moov", &movie)
}

/// Writes a stereo AAC file of silence in an MP4 container, laid out the way a music store's
/// M4A is: file type, then the movie box that describes the audio, then the audio.
pub fn write_silent_m4a(path: &Path, seconds: f64) -> io::Result<()> {
    let frames = u32::try_from(frames_for(seconds, f64::from(AAC_FRAME_SAMPLES)))
        .ok()
        .filter(|frames| frames.checked_mul(AAC_FRAME_SAMPLES).is_some())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "too long for this file format")
        })?;

    let mut file = boxed(b"ftyp", b"M4A \0\0\0\0M4A mp42isom");
    // The movie box says where the audio starts, and its own length sets that position. The
    // length does not depend on the value, so it is built once to be measured.
    let data_offset = (file.len() + movie(frames, 0).len() + 8) as u32;
    file.extend(movie(frames, data_offset));
    file.extend(boxed(b"mdat", &AAC_FRAME.repeat(frames as usize)));
    fs::write(path, file)
}
