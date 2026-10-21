//! Shared event types passed between analysis components.
//!
//! The distinction between an *observation* and a *note* is deliberate:
//!
//! * a [`PitchObservation`] is one analysis window's acoustic measurement —
//!   it may be unvoiced, low-confidence or transitional;
//! * a [`MusicalNoteEvent`] is a confirmed note built from many consistent
//!   observations by the [`NoteTracker`](crate::tracking::NoteTracker).
//!
//! All times are seconds on the producer's monotonic *stream clock* (for an
//! audio pipeline: samples processed ÷ sample rate since the stream
//! started). Producers that need to align with other clocks (e.g. camera
//! frames) must publish the mapping between their stream clock and the
//! host clock.

use crate::pitch::Pitch;
use crate::tuning::NoteMatch;
use serde::{Deserialize, Serialize};

/// Voicing status of one observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObservationStatus {
    /// A periodic signal with an estimated fundamental.
    Voiced,
    /// Signal level below the silence gate.
    Silence,
    /// Signal present but not periodic enough.
    Aperiodic,
    /// Periodic, but outside the configured frequency range.
    OutOfRange,
}

/// One acoustic pitch measurement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PitchObservation {
    /// Stream time of the centre of the analysis window, in seconds.
    pub time_s: f64,
    /// Length of the analysis window in seconds.
    pub window_s: f64,
    /// Voicing status.
    pub status: ObservationStatus,
    /// Fundamental frequency; `Some` only when `status` is `Voiced`.
    pub frequency_hz: Option<f64>,
    /// Detector confidence in `[0, 1]` (a periodicity measure, not a
    /// probability).
    pub confidence: f64,
    /// RMS level of the window in dBFS.
    pub level_db: f64,
}

impl PitchObservation {
    /// Stream time at which the analysis window started.
    pub fn window_start_s(&self) -> f64 {
        self.time_s - self.window_s / 2.0
    }
}

/// Why a note event started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteStart {
    /// After an attack (pluck) detected in the signal.
    Attack,
    /// Pitch moved to a different note without a detected attack (slide,
    /// legato) or the note began without a detectable attack.
    PitchChange,
}

/// A confirmed note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicalNoteEvent {
    /// Identifier, unique within one tracker's lifetime.
    pub id: u64,
    /// Stream time at which the note started, in seconds.
    pub start_s: f64,
    /// Stream time at which it ended; `None` while still sounding.
    pub end_s: Option<f64>,
    /// How the note started.
    pub start_kind: NoteStart,
    /// Median measured frequency over the note so far, in Hz.
    pub frequency_hz: f64,
    /// Median sounding pitch.
    pub sounding_pitch: Pitch,
    /// Median player-relative pitch.
    pub player_pitch: Pitch,
    /// Nearest sounding (concert) note and deviation.
    pub sounding: NoteMatch,
    /// Nearest player-notation note and deviation.
    pub player: NoteMatch,
    /// Mean detector confidence over the note's observations.
    pub confidence: f64,
    /// Highest RMS level reached, dBFS.
    pub peak_level_db: f64,
    /// Number of observations that contributed.
    pub observations: u32,
    /// Instrument course that produced the note, if known (e.g. from
    /// camera evidence). `None` when undetermined.
    #[serde(default)]
    pub course: Option<usize>,
}

impl MusicalNoteEvent {
    /// Duration so far (or total, once ended), relative to `now_s` while
    /// the note is still sounding.
    pub fn duration_s(&self, now_s: f64) -> f64 {
        self.end_s.unwrap_or(now_s) - self.start_s
    }
}
