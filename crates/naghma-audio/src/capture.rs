//! Microphone capture with CPAL.
//!
//! The CPAL data callback runs on a real-time audio thread. It only
//! down-mixes interleaved frames to mono, pushes them into a preallocated
//! lock-free single-producer/single-consumer ring buffer ([`rtrb`]) and
//! updates a few atomics. It never locks, allocates, logs or blocks; when
//! the ring buffer is full, samples are dropped and counted.

use crate::engine::{EngineError, Shared};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

/// An available audio input device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    /// Opaque, stable identifier (pass back to select the device).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Whether this is the system default input.
    pub is_default: bool,
}

/// List input devices on the default host.
pub fn list_input_devices() -> Result<Vec<InputDevice>, EngineError> {
    let host = cpal::default_host();
    let default_id = host.default_input_device().and_then(|d| d.id().ok());
    let devices = host.input_devices().map_err(map_err)?;
    Ok(devices
        .filter_map(|d| {
            let id = d.id().ok()?;
            let name = d
                .description()
                .map(|desc| desc.name().to_string())
                .unwrap_or_else(|_| id.to_string());
            Some(InputDevice {
                is_default: default_id.as_ref() == Some(&id),
                id: id.to_string(),
                name,
            })
        })
        .collect())
}

/// A running capture stream. Dropping it stops capture.
pub(crate) struct Capture {
    stream: cpal::Stream,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) device_name: String,
}

impl Capture {
    pub(crate) fn pause(&self) {
        let _ = self.stream.pause();
    }
}

pub(crate) fn open(
    device_id: Option<&str>,
    producer: rtrb::Producer<f32>,
    shared: Arc<Shared>,
    errors: mpsc::Sender<EngineError>,
) -> Result<Capture, EngineError> {
    let host = cpal::default_host();
    let device = match device_id {
        Some(id) => {
            let parsed: cpal::DeviceId = id
                .parse()
                .map_err(|_| EngineError::DeviceNotFound(id.to_string()))?;
            host.device_by_id(&parsed)
                .ok_or_else(|| EngineError::DeviceNotFound(id.to_string()))?
        }
        None => host
            .default_input_device()
            .ok_or(EngineError::NoInputDevice)?,
    };
    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown input".to_string());
    let supported = device.default_input_config().map_err(map_err)?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let (sample_rate, channels) = (config.sample_rate, config.channels);

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, producer, shared, errors),
        SampleFormat::F64 => build::<f64>(&device, config, producer, shared, errors),
        SampleFormat::I16 => build::<i16>(&device, config, producer, shared, errors),
        SampleFormat::I32 => build::<i32>(&device, config, producer, shared, errors),
        SampleFormat::U16 => build::<u16>(&device, config, producer, shared, errors),
        SampleFormat::I8 => build::<i8>(&device, config, producer, shared, errors),
        SampleFormat::U8 => build::<u8>(&device, config, producer, shared, errors),
        other => return Err(EngineError::UnsupportedFormat(other.to_string())),
    }
    .map_err(map_err)?;
    stream.play().map_err(map_err)?;
    Ok(Capture {
        stream,
        sample_rate,
        channels,
        device_name,
    })
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut producer: rtrb::Producer<f32>,
    shared: Arc<Shared>,
    errors: mpsc::Sender<EngineError>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    let scale = 1.0 / channels as f32;
    let data_shared = shared.clone();
    device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _info| {
            // Real-time context: no locks, no allocation, no I/O.
            let mut pushed = 0u64;
            let mut dropped = 0u64;
            for frame in data.chunks_exact(channels) {
                let mut acc = 0.0f32;
                for &s in frame {
                    acc += f32::from_sample(s);
                }
                if producer.push(acc * scale).is_ok() {
                    pushed += 1;
                } else {
                    dropped += 1;
                }
            }
            data_shared.written.fetch_add(pushed, Ordering::Release);
            if dropped > 0 {
                data_shared.dropped.fetch_add(dropped, Ordering::Relaxed);
            }
            data_shared.mark_write();
        },
        move |err| {
            // Rare path (device lost, etc.); a channel send is acceptable here.
            let _ = errors.send(map_err(err));
        },
        None,
    )
}

fn map_err(e: cpal::Error) -> EngineError {
    use cpal::ErrorKind as K;
    match e.kind() {
        K::PermissionDenied => EngineError::PermissionDenied,
        K::DeviceNotAvailable | K::DeviceChanged | K::StreamInvalidated => {
            EngineError::DeviceUnavailable(e.to_string())
        }
        K::UnsupportedConfig => EngineError::UnsupportedFormat(e.to_string()),
        _ => EngineError::Backend(e.to_string()),
    }
}
