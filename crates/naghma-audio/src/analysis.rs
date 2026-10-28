//! Hardware-independent analysis: samples in, observations and notes out.

use naghma_core::events::{ObservationStatus, PitchObservation};
use naghma_core::interpret::PitchContext;
use naghma_core::tracking::{NoteTracker, TrackerConfig, TrackerUpdate};
use pitchforge::{
    ConfigError, Framer, OnsetConfig, OnsetDetector, PitchDetector, PitchEstimate, UnvoicedReason,
    Voicing, Yin, YinConfig,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Analysis parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalysisConfig {
    /// Analysis window in samples. `0` = choose from the sample rate
    /// (≈ 85 ms, a power of two: 4096 at 44.1/48 kHz).
    pub window: usize,
    /// Hop between windows in samples. `0` = `window / 8`.
    pub hop: usize,
    /// Lowest frequency to detect, Hz.
    pub min_frequency: f64,
    /// Highest frequency to detect, Hz.
    pub max_frequency: f64,
    /// YIN absolute threshold.
    pub yin_threshold: f64,
    /// Silence gate, dBFS RMS.
    pub silence_threshold_db: f64,
    /// Onset detector rise threshold, dB.
    pub onset_rise_db: f64,
    /// Name a plucked note from a shorter window (the newest half of the
    /// analysis window, or less) until the note is shown. A full window
    /// must be mostly past the attack before it reads the new note; a short
    /// one gets there sooner. The full window then confirms the name, or the
    /// tracker renames the note while its attack settles.
    #[serde(default = "yes")]
    pub fast_attack: bool,
}

fn yes() -> bool {
    true
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        let yin = YinConfig::default();
        Self {
            window: 0,
            hop: 0,
            min_frequency: yin.min_frequency,
            max_frequency: yin.max_frequency,
            yin_threshold: yin.threshold,
            silence_threshold_db: yin.silence_threshold_db,
            onset_rise_db: OnsetConfig::default().rise_db,
            fast_attack: true,
        }
    }
}

impl AnalysisConfig {
    fn yin_config(&self) -> YinConfig {
        YinConfig {
            min_frequency: self.min_frequency,
            max_frequency: self.max_frequency,
            threshold: self.yin_threshold,
            voicing_threshold: self
                .yin_threshold
                .max(YinConfig::default().voicing_threshold),
            silence_threshold_db: self.silence_threshold_db,
            ..YinConfig::default()
        }
    }

    /// Window and hop actually used at `sample_rate`.
    ///
    /// An automatic window is ≈ 85 ms rounded up to a power of two (4096 at
    /// 44.1/48 kHz), enlarged if needed to hold two periods of
    /// `min_frequency`.
    pub fn resolved_window_hop(&self, sample_rate: f64) -> (usize, usize) {
        let window = if self.window > 0 {
            self.window
        } else {
            let required = Yin::required_window_size(sample_rate, &self.yin_config()).unwrap_or(0);
            ((sample_rate * 0.085).round() as usize)
                .max(required)
                .max(2048)
                .next_power_of_two()
        };
        let hop = if self.hop > 0 {
            self.hop.min(window)
        } else {
            window / 8
        };
        (window, hop)
    }

    /// Window of the fast attack reading at `sample_rate`, if it is enabled
    /// and shorter than the analysis window: ≈ 40 ms rounded up to a power
    /// of two, at least two periods of `min_frequency`.
    pub fn fast_window(&self, sample_rate: f64) -> Option<usize> {
        let (window, _) = self.resolved_window_hop(sample_rate);
        let required = Yin::required_window_size(sample_rate, &self.yin_config()).ok()?;
        let fast = ((sample_rate * 0.04).round() as usize)
            .max(required)
            .next_power_of_two();
        (self.fast_attack && fast < window).then_some(fast)
    }
}

/// Timing information for one analysis frame.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameTiming {
    /// Time spent in pitch detection + tracking for this frame, µs.
    pub compute_us: f64,
    /// Estimated delay between the newest sample of the window arriving
    /// from the device and this frame being produced, ms. `None` when the
    /// source has no arrival clock (offline analysis).
    pub pipeline_latency_ms: Option<f64>,
    /// Analysis window length, ms. The observation describes the centre of
    /// this window, so it inherently lags the newest audio by half of it.
    pub window_ms: f64,
}

/// Result of analysing one window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFrame {
    /// Acoustic measurement.
    pub observation: PitchObservation,
    /// Note tracker output (display state and note events).
    pub update: TrackerUpdate,
    /// Strength of an onset detected in the newest hop, dB.
    pub onset_strength_db: Option<f64>,
    /// Index of the window's last sample in the stream.
    pub end_sample: u64,
    /// Timing.
    pub timing: FrameTiming,
}

/// Streaming analyser: framing, YIN, onset detection and note tracking.
///
/// Allocation happens at construction; [`Analyzer::push`] allocates only
/// when the tracker starts or ends a note.
#[derive(Debug)]
pub struct Analyzer {
    sample_rate: f64,
    window: usize,
    hop: usize,
    framer: Framer,
    yin: Yin,
    /// Short-window detector and its window, for the fast attack reading.
    fast: Option<(Yin, usize)>,
    onset: OnsetDetector,
    tracker: NoteTracker,
    warmed_up: bool,
    /// Stream time of the newest analysed sample, seconds.
    now_s: f64,
}

impl Analyzer {
    /// Create an analyser for a mono stream at `sample_rate`.
    pub fn new(
        sample_rate: f64,
        config: AnalysisConfig,
        tracker: TrackerConfig,
        context: PitchContext,
    ) -> Result<Self, ConfigError> {
        let (window, hop) = config.resolved_window_hop(sample_rate);
        let yin = Yin::new(sample_rate, window, config.yin_config())?;
        let fast = config
            .fast_window(sample_rate)
            .map(|w| Yin::new(sample_rate, w, config.yin_config()).map(|y| (y, w)))
            .transpose()?;
        let onset = OnsetDetector::new(
            sample_rate,
            OnsetConfig {
                rise_db: config.onset_rise_db,
                ..OnsetConfig::default()
            },
        )?;
        Ok(Self {
            sample_rate,
            window,
            hop,
            framer: Framer::new(window, hop)?,
            yin,
            fast,
            onset,
            tracker: NoteTracker::new(tracker, context),
            warmed_up: false,
            now_s: 0.0,
        })
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Window length in samples.
    pub fn window(&self) -> usize {
        self.window
    }

    /// Hop length in samples.
    pub fn hop(&self) -> usize {
        self.hop
    }

    /// The note tracker (for its context and configuration).
    pub fn tracker(&self) -> &NoteTracker {
        &self.tracker
    }

    /// Change naming context (reference, transposition, tuning). Ends any
    /// current note so no event mixes two contexts.
    pub fn set_context(
        &mut self,
        context: PitchContext,
    ) -> Option<naghma_core::events::MusicalNoteEvent> {
        self.tracker.set_context(context, self.now_s)
    }

    /// Replace tracker parameters, keeping the naming context.
    pub fn set_tracker_config(&mut self, config: TrackerConfig) {
        let context = self.tracker.context().clone();
        self.tracker = NoteTracker::new(config, context);
    }

    /// Feed mono samples; `on_frame` is called for each completed window.
    /// Non-finite windows produce an unvoiced `InvalidInput` observation;
    /// onset history is reset and analysis resumes on the next valid window.
    pub fn push(&mut self, samples: &[f32], mut on_frame: impl FnMut(AnalysisFrame)) {
        let Self {
            sample_rate,
            window,
            hop,
            framer,
            yin,
            fast,
            onset,
            tracker,
            warmed_up,
            now_s,
        } = self;
        let (sr, window, hop) = (*sample_rate, *window, *hop);
        framer.push(samples, |w, start| {
            let t0 = Instant::now();
            *now_s = (start + window as u64) as f64 / sr;

            let estimate = yin.detect(w);

            // Onset detection on the newest hop. On the very first window
            // feed everything so the detector has history.
            let mut onset_hit = None;
            if estimate.is_err() {
                // Do not let a device glitch poison the onset history or
                // panic the DSP worker. Overlapping bad windows stay invalid.
                onset.reset();
                *warmed_up = false;
            } else if !*warmed_up {
                for (i, block) in w.chunks(hop).enumerate() {
                    if let Some(o) = onset.process(block) {
                        onset_hit = Some((start + (i * hop) as u64, o.strength_db));
                    }
                }
                *warmed_up = true;
            } else if let Some(o) = onset.process(&w[window - hop..]) {
                onset_hit = Some((start + (window - hop) as u64, o.strength_db));
            }

            let observe = |estimate: Result<PitchEstimate, _>, length: usize| {
                let mut observation = PitchObservation {
                    time_s: (start as f64 + window as f64 - length as f64 / 2.0) / sr,
                    window_s: length as f64 / sr,
                    status: ObservationStatus::InvalidInput,
                    frequency_hz: None,
                    confidence: 0.0,
                    level_db: -240.0,
                };
                if let Ok(est) = estimate {
                    observation.status = match est.voicing {
                        Voicing::Voiced => ObservationStatus::Voiced,
                        Voicing::Unvoiced(UnvoicedReason::Silence) => ObservationStatus::Silence,
                        Voicing::Unvoiced(UnvoicedReason::OutOfRange) => {
                            ObservationStatus::OutOfRange
                        }
                        Voicing::Unvoiced(_) => ObservationStatus::Aperiodic,
                    };
                    observation.frequency_hz = est.frequency();
                    observation.confidence = est.confidence;
                    observation.level_db = 20.0 * est.diagnostics.rms.max(1e-12).log10();
                }
                observation
            };
            let onset_s = onset_hit.map(|(s, _)| s as f64 / sr);
            let mut observation = observe(estimate, window);
            // Until a plucked note is shown, read it from the newest samples.
            if let Some((fast, length)) = fast.as_mut() {
                let length = *length;
                let latest = (start + window as u64) as f64 / sr;
                if observation.status != ObservationStatus::InvalidInput
                    && (onset_s.is_some() || tracker.awaiting_attack(latest))
                {
                    let short = observe(fast.detect(&w[window - length..]), length);
                    if short.status == ObservationStatus::Voiced {
                        observation = short;
                    }
                }
            }
            let update = tracker.update(&observation, onset_s);
            on_frame(AnalysisFrame {
                observation,
                update,
                onset_strength_db: onset_hit.map(|(_, db)| db),
                end_sample: start + window as u64,
                timing: FrameTiming {
                    compute_us: t0.elapsed().as_secs_f64() * 1e6,
                    pipeline_latency_ms: None,
                    window_ms: window as f64 / sr * 1000.0,
                },
            });
        });
    }

    /// Stream time of the newest analysed sample, seconds.
    pub fn now_s(&self) -> f64 {
        self.now_s
    }

    /// End any sounding note (e.g. at end of stream).
    pub fn flush(&mut self) -> Option<naghma_core::events::MusicalNoteEvent> {
        self.tracker.flush(self.now_s)
    }
}

/// Analyse a complete mono buffer offline.
pub fn analyze_samples(
    samples: &[f32],
    sample_rate: f64,
    config: AnalysisConfig,
    tracker: TrackerConfig,
    context: PitchContext,
) -> Result<Vec<AnalysisFrame>, ConfigError> {
    let mut a = Analyzer::new(sample_rate, config, tracker, context)?;
    let mut frames = Vec::new();
    a.push(samples, |f| frames.push(f));
    Ok(frames)
}
