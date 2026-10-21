//! # naghma-audio
//!
//! Real-time pitch analysis pipeline built on [`pitchforge`] (pitch and
//! onset detection) and [`naghma_core`] (note naming and tracking).
//!
//! * [`analysis`] — hardware-independent [`Analyzer`](analysis::Analyzer):
//!   mono samples in, [`AnalysisFrame`](analysis::AnalysisFrame)s out. Also
//!   usable offline.
//! * [`engine`] — [`Engine`](engine::Engine): a live source (microphone via
//!   CPAL, or a clearly-labelled simulation) feeding a lock-free ring
//!   buffer drained by a dedicated DSP thread.
//! * [`capture`] — input-device listing and the real-time-safe CPAL
//!   callback (`cpal` feature, on by default).
//! * [`simulation`] — synthetic plucked-note source for development.
//! * [`session`] — [`EngineSession`](session::EngineSession), a thread-safe
//!   owner of the running engine for application backends.
//!
//! The crate never sends raw audio to a UI; consumers receive lightweight
//! frames.

pub mod analysis;
#[cfg(feature = "cpal")]
pub mod capture;
pub mod engine;
pub mod session;
pub mod simulation;

pub use analysis::{AnalysisConfig, AnalysisFrame, Analyzer, FrameTiming, analyze_samples};
pub use engine::{
    Engine, EngineConfig, EngineError, EngineEvent, EngineStats, SourceConfig, SourceKind,
    StreamInfo,
};
pub use session::{EngineSession, Reconfigured};
