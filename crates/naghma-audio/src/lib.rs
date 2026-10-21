//! # naghma-audio
//!
//! Real-time pitch analysis pipeline built on [`pitchforge`] (pitch and
//! onset detection) and [`naghma_core`] (note naming and tracking).
//!
//! * [`analysis`] — hardware-independent [`Analyzer`](analysis::Analyzer):
//!   mono samples in, [`AnalysisFrame`](analysis::AnalysisFrame)s out. Also
//!   usable offline.
//!
//! The crate never sends raw audio to a UI; consumers receive lightweight
//! frames.

pub mod analysis;
