use std::fs::{self, File};
use std::path::{Path, PathBuf};

use resonance_fixtures::audio_file::pcm16;
use resonance_fixtures::signal::{ClickTrack, Signal};
use resonance_fixtures::silence::{self, write_silent_m4a, write_silent_mp3};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

struct Decoded {
    sample_rate: u32,
    samples: Vec<i16>,
}

fn decode(path: &Path) -> Decoded {
    let file = Box::new(File::open(path).expect("the fixture file opens"));
    let stream = MediaSourceStream::new(file, Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .expect("Symphonia recognises the file");
    let track = format
        .default_track(TrackType::Audio)
        .expect("the file has an audio track");
    let track_id = track.id;
    let parameters = track
        .codec_params
        .as_ref()
        .and_then(|parameters| parameters.audio())
        .expect("the track has audio parameters")
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&parameters, &AudioDecoderOptions::default())
        .expect("Symphonia has a decoder for the track");

    let mut samples = Vec::new();
    let mut in_packet: Vec<i16> = Vec::new();
    while let Some(packet) = format.next_packet().expect("the next packet reads") {
        if packet.track_id != track_id {
            continue;
        }
        let audio = decoder.decode(&packet).expect("the packet decodes");
        in_packet.resize(audio.samples_interleaved(), 0);
        audio.copy_to_slice_interleaved(&mut in_packet);
        samples.extend_from_slice(&in_packet);
    }
    Decoded {
        sample_rate: parameters.sample_rate.expect("the track has a sample rate"),
        samples,
    }
}

fn fixture_path(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("audio_file");
    fs::create_dir_all(&folder).expect("the test folder can be made");
    folder.join(name)
}

fn as_written(signal: &Signal) -> Vec<i16> {
    signal.samples.iter().copied().map(pcm16).collect()
}

#[test]
fn a_click_track_written_as_wav_decodes_with_every_click_on_its_beat() {
    let clicks = ClickTrack::at_tempo(128.0, 16, 44_100);
    let path = fixture_path("clicks-128.wav");
    clicks.signal.write_wav(&path).expect("the WAV writes");

    let decoded = decode(&path);

    assert_eq!(decoded.sample_rate, 44_100);
    assert_eq!(decoded.samples, as_written(&clicks.signal));
    for &beat in &clicks.beat_frames {
        assert_eq!(decoded.samples[beat], pcm16(0.5));
    }
}

#[test]
fn a_stereo_tone_written_as_aiff_decodes_to_the_same_samples() {
    let tone = Signal::tone_at_loudness(-14.0, 1.0, 48_000);
    let path = fixture_path("tone-minus-14.aiff");
    tone.write_aiff(&path).expect("the AIFF writes");

    let decoded = decode(&path);

    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.samples.len(), 96_000);
    assert_eq!(decoded.samples, as_written(&tone));
}

#[test]
fn the_same_signal_decodes_alike_from_wav_and_aiff_and_a_delay_shifts_it_by_its_frames() {
    let tone = Signal::tone(440.0, 0.5, -6.0, 44_100);
    let delayed = tone.delayed_by(1_000);
    let wav = fixture_path("tone.wav");
    let aiff = fixture_path("tone-delayed.aiff");
    tone.write_wav(&wav).expect("the WAV writes");
    delayed.write_aiff(&aiff).expect("the AIFF writes");

    let original = decode(&wav).samples;
    let shifted = decode(&aiff).samples;

    assert!(shifted[..1_000].iter().all(|&sample| sample == 0));
    assert_eq!(shifted[1_000..], original[..]);
}

#[test]
fn a_click_track_written_as_flac_decodes_to_the_same_samples_across_many_blocks() {
    // Long enough for the frame numbers to need two bytes, with a short block at the end.
    let clicks = ClickTrack::at_tempo(174.0, 36, 44_100);
    let path = fixture_path("clicks-174.flac");
    clicks.signal.write_flac(&path).expect("the FLAC writes");

    let decoded = decode(&path);

    assert!(clicks.signal.frames() > 128 * 4096);
    assert_ne!(clicks.signal.frames() % 4096, 0);
    assert_eq!(decoded.sample_rate, 44_100);
    assert_eq!(decoded.samples, as_written(&clicks.signal));
}

#[test]
fn a_stereo_tone_decodes_alike_from_flac_wav_and_aiff() {
    let tone = Signal::tone_at_loudness(-14.0, 0.5, 48_000);
    let flac = fixture_path("stereo.flac");
    let wav = fixture_path("stereo.wav");
    let aiff = fixture_path("stereo.aiff");
    tone.write_flac(&flac).expect("the FLAC writes");
    tone.write_wav(&wav).expect("the WAV writes");
    tone.write_aiff(&aiff).expect("the AIFF writes");

    let from_flac = decode(&flac).samples;

    assert_eq!(from_flac, as_written(&tone));
    assert_eq!(from_flac, decode(&wav).samples);
    assert_eq!(from_flac, decode(&aiff).samples);
}

#[test]
fn a_silent_mp3_decodes_to_silence_of_about_the_length_asked_for() {
    let path = fixture_path("silence.mp3");
    write_silent_mp3(&path, 2.0).expect("the MP3 writes");

    let decoded = decode(&path);

    let seconds = decoded.samples.len() as f64 / 2.0 / f64::from(silence::SAMPLE_RATE);
    assert_eq!(decoded.sample_rate, silence::SAMPLE_RATE);
    assert!((seconds - 2.0).abs() < 0.1, "decoded {seconds} s");
    assert!(decoded.samples.iter().all(|&sample| sample == 0));
}

#[test]
fn a_silent_m4a_decodes_to_silence_of_about_the_length_asked_for() {
    let path = fixture_path("silence.m4a");
    write_silent_m4a(&path, 2.0).expect("the M4A writes");

    let decoded = decode(&path);

    let seconds = decoded.samples.len() as f64 / 2.0 / f64::from(silence::SAMPLE_RATE);
    assert_eq!(decoded.sample_rate, silence::SAMPLE_RATE);
    assert!((seconds - 2.0).abs() < 0.1, "decoded {seconds} s");
    assert!(decoded.samples.iter().all(|&sample| sample == 0));
}
