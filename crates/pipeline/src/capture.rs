//! Microphone capture.
//!
//! Three threads are involved, and the split matters:
//!
//! 1. **The audio callback** (owned by the OS). Converts the incoming samples to mono
//!    f32 and pushes them into a lock-free queue. No allocation, no locking, no logging.
//! 2. **The capture thread.** Owns the `cpal::Stream` — which is not `Send` on macOS —
//!    and simply keeps it alive until asked to stop.
//! 3. **The resampler thread.** Drains the queue, resamples to 16 kHz, updates the level
//!    meter, and publishes 16 kHz mono audio for the rest of the pipeline.
//!
//! Doing the resampling off the callback keeps the realtime thread cheap and means a
//! slow FFT can never cause a dropout.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use serde::Serialize;

use crate::devices::find_device;
use crate::level::Level;
use crate::resample::{downmix_to_mono, MonoResampler, TARGET_SAMPLE_RATE};
use crate::{Error, Result};

/// Raw queue capacity, in samples at the device rate. Two seconds at 48 kHz — enough to
/// ride out a scheduling hiccup, small enough that a wedged consumer is noticed.
const RAW_QUEUE_SAMPLES: usize = 96_000;

/// How much 16 kHz audio is kept for consumers that fall behind. Thirty seconds is more
/// than the longest speech segment the VAD will ever produce.
const PCM_BUFFER_SAMPLES: usize = TARGET_SAMPLE_RATE as usize * 30;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state", content = "detail")]
pub enum CaptureStatus {
    Stopped,
    Running,
    /// The device went away or the driver failed. Carries the message shown in the UI.
    Failed(String),
}

/// A running microphone capture.
///
/// Dropping this stops the capture and joins its threads.
pub struct Capture {
    device_name: String,
    input_sample_rate: u32,
    input_channels: u16,

    stop: Arc<AtomicBool>,
    status: Arc<Mutex<CaptureStatus>>,
    level_bits: Arc<AtomicU32>,
    pcm: Arc<Mutex<Vec<f32>>>,
    /// Total 16 kHz samples produced since capture started. Lets a consumer notice it
    /// missed audio rather than silently working on a gap.
    produced: Arc<AtomicU32>,

    stream_thread: Option<std::thread::JoinHandle<()>>,
    resampler_thread: Option<std::thread::JoinHandle<()>>,
}

impl Capture {
    /// Open `device_name` (or the system default) and start capturing.
    pub fn start(device_name: Option<&str>) -> Result<Self> {
        let device = find_device(device_name)?;
        let description = device.description().map_err(open_error)?;
        let resolved_name = description.name().to_string();

        // Accept whatever the device prefers rather than demanding a format. Forcing a
        // configuration is a reliable way to fail on perfectly good interfaces.
        let supported = device.default_input_config().map_err(|e| match e.kind() {
            cpal::ErrorKind::PermissionDenied => Error::PermissionDenied,
            _ => Error::UnsupportedFormat {
                device: resolved_name.clone(),
                reason: e.to_string(),
            },
        })?;

        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let input_rate = config.sample_rate;
        let channels = config.channels;

        tracing::info!(
            device = %resolved_name,
            rate = input_rate,
            channels,
            format = ?sample_format,
            "opening microphone"
        );

        let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(RAW_QUEUE_SAMPLES);

        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(CaptureStatus::Running));
        let level_bits = Arc::new(AtomicU32::new(0));
        let pcm = Arc::new(Mutex::new(Vec::with_capacity(PCM_BUFFER_SAMPLES)));
        let produced = Arc::new(AtomicU32::new(0));

        // --- the thread that owns the stream ---------------------------------
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();
        let stream_stop = Arc::clone(&stop);
        let stream_status = Arc::clone(&status);
        let stream_device_name = resolved_name.clone();

        let stream_thread = std::thread::Builder::new()
            .name("dndsound-capture".to_string())
            .spawn(move || {
                let error_status = Arc::clone(&stream_status);
                let error_callback = move |err: cpal::Error| {
                    // A disconnected microphone arrives here. It must be visible, not fatal.
                    tracing::error!(error = %err, "microphone stream error");
                    if let Ok(mut status) = error_status.lock() {
                        *status = CaptureStatus::Failed(err.to_string());
                    }
                };

                let mut mono = Vec::with_capacity(4096);
                let build = build_stream(
                    &device,
                    &config,
                    sample_format,
                    move |samples: &[f32], channel_count: u16| {
                        mono.clear();
                        downmix_to_mono(samples, channel_count, &mut mono);
                        for sample in mono.iter().copied() {
                            // A full queue means the consumer is wedged. Dropping the
                            // newest sample is the only realtime-safe response; the
                            // resampler thread logs it.
                            if producer.push(sample).is_err() {
                                break;
                            }
                        }
                    },
                    error_callback,
                );

                let stream = match build {
                    Ok(stream) => stream,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };

                if let Err(e) = stream.play() {
                    let _ = ready_tx.send(Err(open_error(e)));
                    return;
                }

                let _ = ready_tx.send(Ok(()));

                // The stream is alive only as long as this scope. Park until told to stop.
                while !stream_stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }

                drop(stream);
                if let Ok(mut status) = stream_status.lock() {
                    if !matches!(*status, CaptureStatus::Failed(_)) {
                        *status = CaptureStatus::Stopped;
                    }
                }
                tracing::info!(device = %stream_device_name, "microphone closed");
            })
            .map_err(|e| Error::Open(e.to_string()))?;

        // Surface open failures to the caller instead of leaving a dead capture behind.
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                stop.store(true, Ordering::Relaxed);
                let _ = stream_thread.join();
                return Err(e);
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                let _ = stream_thread.join();
                return Err(Error::Open(format!(
                    "{resolved_name} did not start within five seconds"
                )));
            }
        }

        // --- the thread that resamples ---------------------------------------
        let mut resampler = MonoResampler::new(input_rate)?;
        let resample_stop = Arc::clone(&stop);
        let resample_status = Arc::clone(&status);
        let resample_level = Arc::clone(&level_bits);
        let resample_pcm = Arc::clone(&pcm);
        let resample_produced = Arc::clone(&produced);

        let resampler_thread = std::thread::Builder::new()
            .name("dndsound-resample".to_string())
            .spawn(move || {
                let mut raw = Vec::with_capacity(8192);
                let mut converted = Vec::with_capacity(4096);
                let mut smoothed = Level::default();

                while !resample_stop.load(Ordering::Relaxed) {
                    raw.clear();
                    while let Ok(sample) = consumer.pop() {
                        raw.push(sample);
                    }

                    if raw.is_empty() {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }

                    converted.clear();
                    if let Err(e) = resampler.push(&raw, &mut converted) {
                        tracing::error!(error = %e, "resampling failed");
                        if let Ok(mut status) = resample_status.lock() {
                            *status = CaptureStatus::Failed(e.to_string());
                        }
                        break;
                    }

                    // Meter the resampled audio: it is what the rest of the pipeline sees.
                    smoothed = Level::of(&converted).smoothed(smoothed, 0.6, 0.15);
                    resample_level.store(smoothed.rms.to_bits(), Ordering::Relaxed);

                    if !converted.is_empty() {
                        resample_produced.fetch_add(converted.len() as u32, Ordering::Relaxed);

                        if let Ok(mut buffer) = resample_pcm.lock() {
                            buffer.extend_from_slice(&converted);
                            // Bounded: a consumer that stops reading must not grow memory
                            // for the length of a session.
                            if buffer.len() > PCM_BUFFER_SAMPLES {
                                let excess = buffer.len() - PCM_BUFFER_SAMPLES;
                                buffer.drain(..excess);
                            }
                        }
                    }
                }
            })
            .map_err(|e| Error::Open(e.to_string()))?;

        Ok(Self {
            device_name: resolved_name,
            input_sample_rate: input_rate,
            input_channels: channels,
            stop,
            status,
            level_bits,
            pcm,
            produced,
            stream_thread: Some(stream_thread),
            resampler_thread: Some(resampler_thread),
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn input_sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    pub fn input_channels(&self) -> u16 {
        self.input_channels
    }

    pub fn status(&self) -> CaptureStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(CaptureStatus::Failed("capture state was lost".to_string()))
    }

    /// Smoothed RMS level of the incoming audio, in linear amplitude.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed))
    }

    /// Total 16 kHz samples produced since capture started.
    pub fn produced_samples(&self) -> u32 {
        self.produced.load(Ordering::Relaxed)
    }

    /// Take everything captured since the last call, as 16 kHz mono.
    pub fn take_pcm(&self) -> Vec<f32> {
        match self.pcm.lock() {
            Ok(mut buffer) => std::mem::take(&mut *buffer),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.stream_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.resampler_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Build an input stream for whatever sample format the device speaks, converting each
/// one to f32 before it reaches our code.
fn build_stream<F, E>(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    mut on_samples: F,
    error_callback: E,
) -> Result<cpal::Stream>
where
    F: FnMut(&[f32], u16) + Send + 'static,
    E: FnMut(cpal::Error) + Send + 'static,
{
    let channels = config.channels;
    let config = *config;

    macro_rules! build {
        ($sample:ty, $convert:expr) => {{
            let mut scratch: Vec<f32> = Vec::with_capacity(4096);
            device.build_input_stream(
                config,
                move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                    scratch.clear();
                    scratch.extend(data.iter().copied().map($convert));
                    on_samples(&scratch, channels);
                },
                error_callback,
                None,
            )
        }};
    }

    let built = match format {
        SampleFormat::F32 => build!(f32, |s| s),
        SampleFormat::I16 => build!(i16, |s| s as f32 / i16::MAX as f32),
        SampleFormat::U16 => build!(u16, |s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0),
        SampleFormat::I32 => build!(i32, |s| s as f32 / i32::MAX as f32),
        SampleFormat::I8 => build!(i8, |s| s as f32 / i8::MAX as f32),
        SampleFormat::U8 => build!(u8, |s| (s as f32 / u8::MAX as f32) * 2.0 - 1.0),
        SampleFormat::F64 => build!(f64, |s| s as f32),
        other => {
            return Err(Error::UnsupportedFormat {
                device: String::new(),
                reason: format!("unsupported sample format {other:?}"),
            })
        }
    };

    built.map_err(open_error)
}

/// Translate a cpal failure into something the UI can act on.
///
/// The permission case matters most: on macOS a denied microphone looks like a generic
/// failure unless it is called out, and the fix is a trip to System Settings, not a retry.
fn open_error(err: cpal::Error) -> Error {
    match err.kind() {
        cpal::ErrorKind::PermissionDenied => Error::PermissionDenied,
        cpal::ErrorKind::DeviceNotAvailable => Error::DeviceNotFound(err.to_string()),
        _ => Error::Open(err.to_string()),
    }
}
