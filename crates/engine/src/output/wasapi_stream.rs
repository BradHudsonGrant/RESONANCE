//! WASAPI opened directly, for what cpal's host does not offer: exclusive mode at a set period,
//! and the shortest period the shared mixer allows through `IAudioClient3`.
//!
//! The render loop runs on the output thread. Its buffers are made before it starts, and inside
//! it nothing allocates, locks, logs or does I/O; what goes wrong is counted in [`Published`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use wasapi::{
    AudioClient, AudioRenderClient, DeviceEnumerator, Direction, Handle, SampleType, StreamMode,
    TimingMode, WasapiError, WaveFormat, calculate_period_100ns,
};

use super::{Device, OutputMode, REQUESTED_BUFFER_FRAMES, Renderer};
use crate::published::Published;

/// `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED`: the device wants the period rounded to its own step.
const BUFFER_SIZE_NOT_ALIGNED: i32 = 0x8889_0019_u32 as i32;

/// Events come every period, a few milliseconds apart. A second without one means the device
/// has gone.
const EVENT_TIMEOUT_MS: u32 = 1000;

/// Sample layouts tried for exclusive use, best first. The shared mixer always takes floats.
const EXCLUSIVE_LAYOUTS: [Layout; 4] = [Layout::F32, Layout::I32, Layout::I24, Layout::I16];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    F32,
    /// 24 valid bits in a 32-bit container.
    I32,
    /// 24 bits packed into three bytes.
    I24,
    I16,
}

impl Layout {
    fn format(self, sample_rate: u32, channels: u16) -> WaveFormat {
        let (container, valid, kind) = match self {
            Self::F32 => (32, 32, SampleType::Float),
            Self::I32 => (32, 24, SampleType::Int),
            Self::I24 => (24, 24, SampleType::Int),
            Self::I16 => (16, 16, SampleType::Int),
        };
        WaveFormat::new(
            container,
            valid,
            kind,
            sample_rate as usize,
            usize::from(channels),
            None,
        )
    }

    fn bytes(self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::I24 => 3,
            Self::I16 => 2,
        }
    }

    fn write(self, samples: &[f32], bytes: &mut [u8]) {
        match self {
            Self::F32 => {
                for (sample, out) in samples.iter().zip(bytes.as_chunks_mut::<4>().0) {
                    *out = sample.to_le_bytes();
                }
            }
            Self::I32 => {
                for (sample, out) in samples.iter().zip(bytes.as_chunks_mut::<4>().0) {
                    let value = (sample.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
                    *out = (value << 8).to_le_bytes();
                }
            }
            Self::I24 => {
                for (sample, out) in samples.iter().zip(bytes.as_chunks_mut::<3>().0) {
                    let [low, middle, high, _] =
                        ((sample.clamp(-1.0, 1.0) * 8_388_607.0) as i32).to_le_bytes();
                    *out = [low, middle, high];
                }
            }
            Self::I16 => {
                for (sample, out) in samples.iter().zip(bytes.as_chunks_mut::<2>().0) {
                    *out = ((sample.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes();
                }
            }
        }
    }
}

pub(super) struct Stream {
    client: AudioClient,
    render: AudioRenderClient,
    event: Handle,
    layout: Layout,
    buffer_frames: u32,
    stream_latency: Duration,
    /// An exclusive stream is already running on silence from settling.
    started: bool,
    _priority: Option<audio_thread_priority::RtPriorityHandle>,
    pub device: Device,
}

impl Stream {
    /// Opens the default output device in `mode`, or shared if exclusive use is refused.
    pub fn open(mode: OutputMode, published: &Published) -> Result<Self, WasapiError> {
        // The output thread is the engine's own, so it can join the multithreaded apartment.
        let _ = wasapi::initialize_mta();
        let enumerator = DeviceEnumerator::new()?;
        let endpoint = enumerator.get_default_device(Direction::Render)?;
        let name = endpoint
            .get_friendlyname()
            .unwrap_or_else(|_| "Default output".to_owned());
        let mix = endpoint.get_iaudioclient()?.get_mixformat()?;
        let sample_rate = mix.get_samplespersec();
        let channels = mix.get_nchannels();

        // Held for the life of the stream, settling included: dropping it hands the thread back
        // to normal scheduling.
        let priority = audio_thread_priority::promote_current_thread_to_real_time(
            REQUESTED_BUFFER_FRAMES,
            sample_rate,
        )
        .ok();

        let exclusive = match mode {
            OutputMode::Exclusive => open_steady_exclusive(&endpoint, sample_rate, channels),
            OutputMode::Shared => None,
        };
        let opened = match exclusive {
            Some(opened) => opened,
            None => open_shared(&endpoint, &mix)?,
        };
        let buffer_frames = opened.client.get_buffer_size()?;
        let stream_latency = opened
            .client
            .get_stream_latency()
            .map(|hns| Duration::from_nanos(u64::try_from(hns).unwrap_or(0) * 100))
            .unwrap_or_default();
        published
            .counters
            .period_frames
            .store(opened.period_frames, Ordering::Relaxed);
        Ok(Self {
            client: opened.client,
            render: opened.render,
            event: opened.event,
            layout: opened.layout,
            buffer_frames,
            stream_latency,
            started: opened.started,
            _priority: priority,
            device: Device {
                name,
                sample_rate,
                channels,
                mode: opened.mode,
                requested_buffer_frames: REQUESTED_BUFFER_FRAMES,
                period_frames: opened.period_frames,
            },
        })
    }

    /// Feeds the device from `renderer` until `stopping` is set or the device goes.
    pub fn run(self, mut renderer: Renderer, stopping: &AtomicBool) {
        let channels = usize::from(self.device.channels.max(1));
        let sample_rate = self.device.sample_rate.max(1);
        renderer.set_format(sample_rate, channels);
        let exclusive = self.device.mode == OutputMode::Exclusive;
        if exclusive {
            renderer.event_period = Some(frames_to_duration(self.buffer_frames, sample_rate));
        }
        let mut buffers = Buffers {
            samples: vec![0.0f32; self.buffer_frames as usize * channels],
            bytes: vec![0u8; self.buffer_frames as usize * self.layout.bytes() * channels],
            channels,
        };
        let published = std::sync::Arc::clone(&renderer.published);
        let errors = &published.counters.stream_errors;

        // The device plays whatever is in its buffer when it starts, so the first callback fills
        // all of it, as cpal's does.
        if !self.started {
            let first = self.fill(&mut renderer, &mut buffers, 0);
            if first.is_err() || self.client.start_stream().is_err() {
                errors.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        while !stopping.load(Ordering::Acquire) {
            if self.event.wait_for_event(EVENT_TIMEOUT_MS).is_err() {
                errors.fetch_add(1, Ordering::Relaxed);
                break;
            }
            // An exclusive device swaps whole buffers: one plays while the other is written.
            let queued = if exclusive {
                Ok(self.buffer_frames)
            } else {
                self.client.get_current_padding()
            };
            let filled = queued.and_then(|queued| self.fill(&mut renderer, &mut buffers, queued));
            if filled.is_err() {
                errors.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
        let _ = self.client.stop_stream();
    }

    /// Renders and writes one callback's worth, given the frames still waiting to be played.
    fn fill(
        &self,
        renderer: &mut Renderer,
        buffers: &mut Buffers,
        queued: u32,
    ) -> Result<(), WasapiError> {
        let frames = if self.device.mode == OutputMode::Exclusive {
            self.buffer_frames
        } else {
            self.buffer_frames.saturating_sub(queued)
        } as usize;
        if frames == 0 {
            return Ok(());
        }
        let latency = frames_to_duration(queued, self.device.sample_rate) + self.stream_latency;
        let out = &mut buffers.samples[..frames * buffers.channels];
        renderer.render_with_latency(out, latency);
        let encoded = &mut buffers.bytes[..frames * buffers.channels * self.layout.bytes()];
        self.layout.write(out, encoded);
        self.render.write_to_device(encoded, None)
    }
}

/// Made once before the loop, at the size of the device's whole buffer.
struct Buffers {
    samples: Vec<f32>,
    bytes: Vec<u8>,
    channels: usize,
}

struct Opened {
    client: AudioClient,
    render: AudioRenderClient,
    event: Handle,
    layout: Layout,
    period_frames: u32,
    mode: OutputMode,
    started: bool,
}

impl Opened {
    fn new(
        client: AudioClient,
        layout: Layout,
        period_frames: u32,
        mode: OutputMode,
    ) -> Result<Self, WasapiError> {
        let event = client.set_get_eventhandle()?;
        let render = client.get_audiorenderclient()?;
        Ok(Self {
            client,
            render,
            event,
            layout,
            period_frames,
            mode,
            started: false,
        })
    }
}

/// Exclusive streams tried before falling back to shared.
const EXCLUSIVE_ATTEMPTS: usize = 2;

/// How long a new exclusive stream plays silence while its timing is checked.
const SETTLE: Duration = Duration::from_millis(500);

/// Opens the device for exclusive use and plays silence until it is clear the device is asking
/// once a period. Some opens come up asking only every other period, which is heard as constant
/// crackle; such a stream is closed and the device opened again, and then shared.
fn open_steady_exclusive(
    endpoint: &wasapi::Device,
    sample_rate: u32,
    channels: u16,
) -> Option<Opened> {
    for _ in 0..EXCLUSIVE_ATTEMPTS {
        let Ok(mut opened) = open_exclusive(endpoint, sample_rate, channels) else {
            return None;
        };
        if settles(&opened, sample_rate, usize::from(channels)).unwrap_or(false) {
            opened.started = true;
            return Some(opened);
        }
        let _ = opened.client.stop_stream();
    }
    None
}

/// Starts `opened` on silence for [`SETTLE`] and says whether its callbacks came on time.
fn settles(opened: &Opened, sample_rate: u32, channels: usize) -> Result<bool, WasapiError> {
    let period = frames_to_duration(opened.period_frames, sample_rate);
    let silence = vec![0u8; opened.period_frames as usize * channels * opened.layout.bytes()];
    opened.render.write_to_device(&silence, None)?;
    opened.client.start_stream()?;
    let started = Instant::now();
    let mut last = started;
    let mut callbacks = 0;
    let mut late = 0;
    while started.elapsed() < SETTLE {
        opened.event.wait_for_event(EVENT_TIMEOUT_MS)?;
        let now = Instant::now();
        if (now - last) * 2 > period * 3 {
            late += 1;
        }
        last = now;
        callbacks += 1;
        opened.render.write_to_device(&silence, None)?;
    }
    Ok(on_time(callbacks, late))
}

/// A healthy stream has the odd late callback; one in ten is a device that has fallen behind.
fn on_time(callbacks: u32, late: u32) -> bool {
    callbacks > 0 && late * 10 <= callbacks
}

fn open_exclusive(
    endpoint: &wasapi::Device,
    sample_rate: u32,
    channels: u16,
) -> Result<Opened, WasapiError> {
    let probe = endpoint.get_iaudioclient()?;
    let (layout, format) = EXCLUSIVE_LAYOUTS
        .iter()
        .find_map(|layout| {
            probe
                .is_supported_exclusive_with_quirks(&layout.format(sample_rate, channels))
                .ok()
                .map(|format| (*layout, format))
        })
        .ok_or(WasapiError::UnsupportedFormat)?;
    let (_, min_period) = probe.get_device_period()?;
    let wanted = calculate_period_100ns(i64::from(REQUESTED_BUFFER_FRAMES), i64::from(sample_rate))
        .max(min_period);

    let mut client = endpoint.get_iaudioclient()?;
    let first = client.initialize_client(
        &format,
        Direction::Render,
        StreamMode::EventsExclusive { period_hns: wanted },
        None,
    );
    match first {
        Ok(()) => {}
        Err(WasapiError::Windows(error)) if error.code().0 == BUFFER_SIZE_NOT_ALIGNED => {
            // The failed client reports the aligned size; a fresh client takes that instead.
            let aligned = calculate_period_100ns(
                i64::from(client.get_buffer_size()?),
                i64::from(sample_rate),
            );
            client = endpoint.get_iaudioclient()?;
            client.initialize_client(
                &format,
                Direction::Render,
                StreamMode::EventsExclusive {
                    period_hns: aligned,
                },
                None,
            )?;
        }
        Err(error) => return Err(error),
    }
    let period_frames = client.get_buffer_size()?;
    Opened::new(client, layout, period_frames, OutputMode::Exclusive)
}

fn open_shared(endpoint: &wasapi::Device, mix: &WaveFormat) -> Result<Opened, WasapiError> {
    if mix.get_subformat()? != SampleType::Float || mix.get_bitspersample() != 32 {
        return Err(WasapiError::UnsupportedFormat);
    }
    let mut client = endpoint.get_iaudioclient()?;
    let periods = client.get_shared_mode_engine_period(mix)?;
    let step = periods.fundamental.max(1);
    let period_frames = (REQUESTED_BUFFER_FRAMES / step * step).clamp(periods.min, periods.max);
    client.initialize_shared_audio_stream(mix, TimingMode::Events, period_frames)?;
    Opened::new(client, Layout::F32, period_frames, OutputMode::Shared)
}

fn frames_to_duration(frames: u32, sample_rate: u32) -> Duration {
    Duration::from_nanos(u64::from(frames) * 1_000_000_000 / u64::from(sample_rate.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_layout_writes_full_scale_little_endian() {
        let samples = [1.0f32, -1.0];
        let mut i16_bytes = [0u8; 4];
        Layout::I16.write(&samples, &mut i16_bytes);
        assert_eq!(i16_bytes, [0xff, 0x7f, 0x01, 0x80]);

        let mut i24_bytes = [0u8; 6];
        Layout::I24.write(&samples, &mut i24_bytes);
        assert_eq!(i24_bytes, [0xff, 0xff, 0x7f, 0x01, 0x00, 0x80]);

        let mut i32_bytes = [0u8; 8];
        Layout::I32.write(&samples, &mut i32_bytes);
        assert_eq!(i32_bytes, [0x00, 0xff, 0xff, 0x7f, 0x00, 0x01, 0x00, 0x80]);

        let mut f32_bytes = [0u8; 8];
        Layout::F32.write(&[0.5], &mut f32_bytes[..4]);
        assert_eq!(f32_bytes[..4], 0.5f32.to_le_bytes());
    }

    #[test]
    fn a_stream_with_one_late_callback_in_ten_or_more_has_not_settled() {
        assert!(on_time(94, 1));
        assert!(on_time(94, 9));
        assert!(!on_time(94, 10));
        // An open that asks every other period: about half the callbacks, all of them late.
        assert!(!on_time(47, 47));
        assert!(!on_time(0, 0));
    }

    #[test]
    fn samples_past_full_scale_are_clipped_rather_than_wrapped() {
        let mut bytes = [0u8; 2];
        Layout::I16.write(&[1.5], &mut bytes);
        assert_eq!(i16::from_le_bytes(bytes), i16::MAX);
    }
}
