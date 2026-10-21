//! The live analysis engine: an audio source feeding a dedicated DSP thread.
//!
//! ```text
//! ┌──────────────┐  mono f32   ┌──────────────┐   ┌─────────────────────┐
//! │ CPAL callback │──► SPSC ──►│  DSP thread  │──►│ on_event(Frame)     │
//! │ or simulation │   ring buf  │  Analyzer    │   │ (app forwards to UI)│
//! └──────────────┘             └──────────────┘   └─────────────────────┘
//! ```
//!
//! The DSP thread owns the capture stream (CPAL streams are not `Send` on
//! every platform), drains the ring buffer, runs the
//! [`Analyzer`](crate::analysis::Analyzer) and calls the application's
//! callback with lightweight [`AnalysisFrame`]s — raw audio never leaves
//! the engine.

use crate::analysis::{AnalysisConfig, AnalysisFrame, Analyzer};
use crate::simulation::SimulationSpec;
use naghma_core::events::MusicalNoteEvent;
use naghma_core::interpret::PitchContext;
use naghma_core::tracking::TrackerConfig;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Where audio comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SourceConfig {
    /// A microphone / audio interface. `device_id` from
    /// [`list_input_devices`](crate::capture::list_input_devices); `None`
    /// for the system default.
    Microphone {
        /// Device identifier.
        #[serde(default)]
        device_id: Option<String>,
    },
    /// Synthetic plucked notes. **Not live audio.**
    Simulation(SimulationSpec),
}

/// Kind of the running source, for labelling in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    /// Live audio from an input device.
    Microphone,
    /// Synthetic test signal.
    Simulation,
}

/// Engine start-up parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineConfig {
    /// Audio source.
    pub source: SourceConfig,
    /// Analysis parameters.
    #[serde(default)]
    pub analysis: AnalysisConfig,
    /// Note-tracker parameters.
    #[serde(default)]
    pub tracker: TrackerConfig,
    /// Naming context.
    #[serde(default)]
    pub context: PitchContext,
}

/// Facts about the running stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    /// Microphone or simulation.
    pub source: SourceKind,
    /// Device (or simulation) name.
    pub device_name: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channels delivered by the device (down-mixed to mono).
    pub channels: u16,
    /// Analysis window in samples.
    pub window: usize,
    /// Hop in samples.
    pub hop: usize,
}

/// Counters for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStats {
    /// Samples delivered by the source into the ring buffer.
    pub received_samples: u64,
    /// Samples dropped because the ring buffer was full (DSP too slow).
    pub dropped_samples: u64,
    /// Samples analysed by the DSP thread.
    pub analysed_samples: u64,
    /// Samples waiting in the ring buffer.
    pub buffered_samples: u64,
    /// Ring buffer capacity in samples.
    pub buffer_capacity: u64,
}

/// Something the engine reports to the application.
// `Frame` is by far the most frequent variant; boxing it would allocate on
// every hop just to shrink the rare variants.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum EngineEvent {
    /// One analysed window.
    Frame(AnalysisFrame),
    /// A note ended outside normal frame processing (context change or
    /// stop).
    NoteEnded(MusicalNoteEvent),
    /// A runtime error from the source (e.g. device unplugged).
    Error {
        /// The error.
        error: EngineError,
    },
}

/// Engine errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "code", content = "detail")]
#[non_exhaustive]
pub enum EngineError {
    /// No input device available.
    NoInputDevice,
    /// The requested device id does not exist.
    DeviceNotFound(String),
    /// The OS denied microphone access.
    PermissionDenied,
    /// The device disappeared or changed.
    DeviceUnavailable(String),
    /// The device's sample format/config is not supported.
    UnsupportedFormat(String),
    /// Invalid analysis configuration.
    Config(String),
    /// Microphone capture was not compiled in (`cpal` feature disabled).
    CaptureUnavailable,
    /// The audio system did not finish opening the device in time. On
    /// macOS this usually means a microphone-permission prompt is waiting
    /// for the user.
    StartTimeout,
    /// Other backend error.
    Backend(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputDevice => write!(f, "no audio input device is available"),
            Self::DeviceNotFound(id) => write!(f, "audio input device not found: {id}"),
            Self::PermissionDenied => write!(
                f,
                "microphone access was denied; allow it in the system privacy settings"
            ),
            Self::DeviceUnavailable(e) => write!(f, "audio device unavailable: {e}"),
            Self::UnsupportedFormat(e) => write!(f, "unsupported audio format: {e}"),
            Self::Config(e) => write!(f, "invalid analysis configuration: {e}"),
            Self::CaptureUnavailable => {
                write!(f, "microphone capture is not available in this build")
            }
            Self::StartTimeout => write!(
                f,
                "the audio device did not start in time; a microphone permission prompt may be waiting"
            ),
            Self::Backend(e) => write!(f, "audio backend error: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// State shared between the source (producer) side and the DSP thread.
#[derive(Debug)]
pub(crate) struct Shared {
    epoch: Instant,
    pub(crate) written: AtomicU64,
    pub(crate) dropped: AtomicU64,
    analysed: AtomicU64,
    /// Nanoseconds since `epoch` at the end of the latest source write.
    last_write_ns: AtomicU64,
    capacity: u64,
}

impl Shared {
    fn new(capacity: usize) -> Self {
        Self {
            epoch: Instant::now(),
            written: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            analysed: AtomicU64::new(0),
            last_write_ns: AtomicU64::new(0),
            capacity: capacity as u64,
        }
    }

    /// Record that a write just finished. Real-time safe.
    pub(crate) fn mark_write(&self) {
        let ns = self.epoch.elapsed().as_nanos() as u64;
        self.last_write_ns.store(ns, Ordering::Release);
    }

    /// Estimated ms since sample `index` arrived.
    fn latency_ms(&self, index: u64, sample_rate: f64) -> Option<f64> {
        let written = self.written.load(Ordering::Acquire);
        let last = self.last_write_ns.load(Ordering::Acquire);
        if written == 0 || index > written {
            return None;
        }
        let behind_ns = (written - index) as f64 / sample_rate * 1e9;
        let arrival_ns = last as f64 - behind_ns;
        let now_ns = self.epoch.elapsed().as_nanos() as f64;
        Some(((now_ns - arrival_ns) / 1e6).max(0.0))
    }
}

enum Command {
    SetContext(PitchContext),
    SetTracker(TrackerConfig),
}

/// A running analysis engine. Dropping it stops the stream.
pub struct Engine {
    info: StreamInfo,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

/// How long [`Engine::start`] waits for the source to open.
pub const DEFAULT_START_TIMEOUT: Duration = Duration::from_secs(20);

/// Ring buffer length in seconds of audio. Generous so a briefly stalled
/// DSP thread does not drop samples; the DSP thread normally keeps it
/// nearly empty.
const RING_SECONDS: f64 = 2.0;

impl Engine {
    /// Start capturing and analysing. `on_event` runs on the DSP thread;
    /// keep it cheap (e.g. forward to a channel or IPC).
    ///
    /// Returns once the source is running, or with the error that prevented
    /// it from starting (e.g. [`EngineError::PermissionDenied`]). Gives up
    /// after [`DEFAULT_START_TIMEOUT`] with [`EngineError::StartTimeout`].
    pub fn start(
        config: EngineConfig,
        on_event: impl FnMut(EngineEvent) + Send + 'static,
    ) -> Result<Engine, EngineError> {
        Self::start_with_timeout(config, DEFAULT_START_TIMEOUT, on_event)
    }

    /// [`Engine::start`] with an explicit start-up timeout.
    ///
    /// Opening a microphone can block inside the OS while a permission
    /// prompt is shown. On timeout the half-started engine is told to shut
    /// down as soon as the OS returns control, and no events are delivered.
    pub fn start_with_timeout(
        config: EngineConfig,
        timeout: Duration,
        mut on_event: impl FnMut(EngineEvent) + Send + 'static,
    ) -> Result<Engine, EngineError> {
        let (ready_tx, ready_rx) =
            mpsc::sync_channel::<Result<(StreamInfo, Arc<Shared>), EngineError>>(1);
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_dsp = stop.clone();

        let thread = thread::Builder::new()
            .name("naghma-dsp".into())
            .spawn(move || {
                let (err_tx, err_rx) = mpsc::channel::<EngineError>();
                let source = match open_source(&config.source, err_tx, stop_dsp.clone()) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let mut analyzer = match Analyzer::new(
                    source.sample_rate as f64,
                    config.analysis,
                    config.tracker,
                    config.context,
                ) {
                    Ok(a) => a,
                    Err(e) => {
                        let _ = ready_tx.send(Err(EngineError::Config(e.to_string())));
                        return;
                    }
                };
                let info = StreamInfo {
                    source: source.kind,
                    device_name: source.device_name.clone(),
                    sample_rate: source.sample_rate,
                    channels: source.channels,
                    window: analyzer.window(),
                    hop: analyzer.hop(),
                };
                let shared = source.shared.clone();
                if ready_tx.send(Ok((info, shared.clone()))).is_err() {
                    return;
                }
                run_dsp(
                    source,
                    &mut analyzer,
                    &shared,
                    &stop_dsp,
                    &cmd_rx,
                    &err_rx,
                    &mut on_event,
                );
                if let Some(e) = analyzer.flush() {
                    on_event(EngineEvent::NoteEnded(e));
                }
            })
            .map_err(|e| EngineError::Backend(e.to_string()))?;

        match ready_rx.recv_timeout(timeout) {
            Ok(Ok((info, shared))) => Ok(Engine {
                info,
                shared,
                stop,
                commands: cmd_tx,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // The thread is blocked in the OS; it will see the stop flag
                // (and a closed ready channel) once it returns. Don't join.
                stop.store(true, Ordering::Release);
                Err(EngineError::StartTimeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
                Err(EngineError::Backend(
                    "DSP thread exited during start-up".into(),
                ))
            }
        }
    }

    /// Stream facts.
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Diagnostic counters.
    pub fn stats(&self) -> EngineStats {
        let received = self.shared.written.load(Ordering::Acquire);
        let analysed = self.shared.analysed.load(Ordering::Acquire);
        EngineStats {
            received_samples: received,
            dropped_samples: self.shared.dropped.load(Ordering::Relaxed),
            analysed_samples: analysed,
            buffered_samples: received.saturating_sub(analysed),
            buffer_capacity: self.shared.capacity,
        }
    }

    /// Change naming context (reference, transposition, tuning) without
    /// restarting the stream.
    pub fn set_context(&self, context: PitchContext) {
        let _ = self.commands.send(Command::SetContext(context));
    }

    /// Change note-tracker parameters without restarting the stream.
    pub fn set_tracker_config(&self, config: TrackerConfig) {
        let _ = self.commands.send(Command::SetTracker(config));
    }

    /// Stop the stream and wait for the DSP thread to finish.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct OpenSource {
    kind: SourceKind,
    device_name: String,
    sample_rate: u32,
    channels: u16,
    consumer: rtrb::Consumer<f32>,
    shared: Arc<Shared>,
    #[cfg(feature = "cpal")]
    capture: Option<crate::capture::Capture>,
    generator: Option<JoinHandle<()>>,
}

fn open_source(
    source: &SourceConfig,
    errors: mpsc::Sender<EngineError>,
    stop: Arc<AtomicBool>,
) -> Result<OpenSource, EngineError> {
    match source {
        SourceConfig::Microphone { device_id } => open_microphone(device_id.as_deref(), errors),
        SourceConfig::Simulation(spec) => {
            let _ = errors;
            let capacity = (spec.sample_rate as f64 * RING_SECONDS) as usize;
            let (producer, consumer) = rtrb::RingBuffer::new(capacity);
            let shared = Arc::new(Shared::new(capacity));
            let generator = spawn_simulation(spec.clone(), producer, shared.clone(), stop)?;
            Ok(OpenSource {
                kind: SourceKind::Simulation,
                device_name: "Simulation (synthetic plucked notes)".into(),
                sample_rate: spec.sample_rate,
                channels: 1,
                consumer,
                shared,
                #[cfg(feature = "cpal")]
                capture: None,
                generator: Some(generator),
            })
        }
    }
}

#[cfg(feature = "cpal")]
fn open_microphone(
    device_id: Option<&str>,
    errors: mpsc::Sender<EngineError>,
) -> Result<OpenSource, EngineError> {
    // Size the ring for the highest common rate; reallocation is not
    // possible once the callback owns the producer.
    let capacity = (192_000.0 * RING_SECONDS) as usize;
    let (producer, consumer) = rtrb::RingBuffer::new(capacity);
    let shared = Arc::new(Shared::new(capacity));
    let capture = crate::capture::open(device_id, producer, shared.clone(), errors)?;
    Ok(OpenSource {
        kind: SourceKind::Microphone,
        device_name: capture.device_name.clone(),
        sample_rate: capture.sample_rate,
        channels: capture.channels,
        consumer,
        shared,
        capture: Some(capture),
        generator: None,
    })
}

#[cfg(not(feature = "cpal"))]
fn open_microphone(
    _device_id: Option<&str>,
    _errors: mpsc::Sender<EngineError>,
) -> Result<OpenSource, EngineError> {
    Err(EngineError::CaptureUnavailable)
}

/// Feed the ring buffer with a looping simulated signal at real-time pace.
fn spawn_simulation(
    spec: SimulationSpec,
    mut producer: rtrb::Producer<f32>,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, EngineError> {
    let audio = spec.render();
    let sr = spec.sample_rate as f64;
    thread::Builder::new()
        .name("naghma-sim".into())
        .spawn(move || {
            const BLOCK: usize = 256;
            let start = Instant::now();
            let mut sent: u64 = 0;
            let mut pos = 0usize;
            while !stop.load(Ordering::Acquire) {
                // Pace to wall-clock time like a real device would.
                let due = (start.elapsed().as_secs_f64() * sr) as u64;
                while sent + BLOCK as u64 <= due {
                    let mut pushed = 0;
                    for _ in 0..BLOCK {
                        if producer.push(audio[pos]).is_ok() {
                            pushed += 1;
                        } else {
                            shared.dropped.fetch_add(1, Ordering::Relaxed);
                        }
                        pos = (pos + 1) % audio.len();
                    }
                    sent += BLOCK as u64;
                    shared.written.fetch_add(pushed, Ordering::Release);
                    shared.mark_write();
                }
                thread::sleep(Duration::from_millis(2));
            }
        })
        .map_err(|e| EngineError::Backend(e.to_string()))
}

fn run_dsp(
    mut source: OpenSource,
    analyzer: &mut Analyzer,
    shared: &Shared,
    stop: &AtomicBool,
    commands: &mpsc::Receiver<Command>,
    errors: &mpsc::Receiver<EngineError>,
    on_event: &mut impl FnMut(EngineEvent),
) {
    let sr = analyzer.sample_rate();
    let mut buf = vec![0.0f32; 8192];
    while !stop.load(Ordering::Acquire) {
        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                Command::SetContext(ctx) => {
                    if let Some(e) = analyzer.set_context(ctx) {
                        on_event(EngineEvent::NoteEnded(e));
                    }
                }
                Command::SetTracker(cfg) => analyzer.set_tracker_config(cfg),
            }
        }
        while let Ok(error) = errors.try_recv() {
            on_event(EngineEvent::Error { error });
        }

        let available = source.consumer.slots().min(buf.len());
        if available == 0 {
            // Polling interval well below one hop (≈10 ms at 48 kHz).
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        let chunk = source
            .consumer
            .read_chunk(available)
            .expect("slots() reported this many");
        let (a, b) = chunk.as_slices();
        buf[..a.len()].copy_from_slice(a);
        buf[a.len()..a.len() + b.len()].copy_from_slice(b);
        chunk.commit_all();
        analyzer.push(&buf[..available], |mut frame| {
            frame.timing.pipeline_latency_ms = shared.latency_ms(frame.end_sample, sr);
            on_event(EngineEvent::Frame(frame));
        });
        shared
            .analysed
            .fetch_add(available as u64, Ordering::Release);
    }

    #[cfg(feature = "cpal")]
    if let Some(c) = &source.capture {
        c.pause();
    }
    if let Some(g) = source.generator.take() {
        let _ = g.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naghma_core::tracking::LiveState;
    use std::sync::Mutex;

    #[test]
    fn simulation_engine_produces_labelled_frames_and_notes() {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let sink = frames.clone();
        let engine = Engine::start(
            EngineConfig {
                source: SourceConfig::Simulation(SimulationSpec::default()),
                analysis: AnalysisConfig::default(),
                tracker: TrackerConfig::default(),
                context: PitchContext::default(),
            },
            move |e| {
                if let EngineEvent::Frame(f) = e {
                    sink.lock().unwrap().push(f);
                }
            },
        )
        .unwrap();
        assert_eq!(engine.info().source, SourceKind::Simulation);
        assert_eq!(engine.info().window, 4096);
        thread::sleep(Duration::from_millis(1600));
        let stats = engine.stats();
        engine.stop();

        let frames = frames.lock().unwrap();
        assert!(frames.len() > 80, "only {} frames", frames.len());
        assert!(stats.received_samples > 48_000);
        assert_eq!(stats.dropped_samples, 0);
        let notes: Vec<String> = frames
            .iter()
            .filter_map(|f| f.update.started.as_ref())
            .map(|e| e.player.note.to_string())
            .collect();
        assert!(
            notes.starts_with(&["Re3".to_string(), "Mi half-flat 3".to_string()]),
            "{notes:?}"
        );
        assert!(
            frames
                .iter()
                .any(|f| matches!(f.update.state, LiveState::Note(_)))
        );
        let lat: Vec<f64> = frames
            .iter()
            .filter_map(|f| f.timing.pipeline_latency_ms)
            .collect();
        assert!(!lat.is_empty());
        let max = lat.iter().cloned().fold(0.0, f64::max);
        assert!(max < 100.0, "pipeline latency up to {max} ms");
    }

    #[test]
    fn context_changes_apply_live() {
        let (tx, rx) = mpsc::channel();
        let engine = Engine::start(
            EngineConfig {
                source: SourceConfig::Simulation(SimulationSpec {
                    notes: vec![crate::simulation::SimNote {
                        frequency_hz: 261.63,
                        duration_s: 0.5,
                    }],
                    ..SimulationSpec::default()
                }),
                analysis: AnalysisConfig::default(),
                tracker: TrackerConfig::default(),
                context: PitchContext::default(),
            },
            move |e| {
                let _ = tx.send(e);
            },
        )
        .unwrap();
        thread::sleep(Duration::from_millis(400));
        engine.set_context(PitchContext {
            transposition: naghma_core::pitch::Transposition::new(
                -2.0,
                naghma_core::pitch::IntervalUnit::Semitone,
            ),
            ..PitchContext::default()
        });
        thread::sleep(Duration::from_millis(900));
        engine.stop();
        let names: Vec<String> = rx
            .try_iter()
            .filter_map(|e| match e {
                EngineEvent::Frame(f) => f.update.started.map(|s| s.player.note.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(names.first().map(String::as_str), Some("Do4"));
        assert!(names.iter().any(|n| n == "Re4"), "{names:?}");
    }
}
