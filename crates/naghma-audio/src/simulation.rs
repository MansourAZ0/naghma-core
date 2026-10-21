//! Synthetic input for development without a microphone or instrument.
//!
//! **Simulation is never live audio.** The engine reports
//! [`SourceKind::Simulation`](crate::engine::SourceKind::Simulation) for
//! streams from this module and applications must label them clearly.

use naghma_core::notation::SpelledNote;
use naghma_core::pitch::ReferencePitch;
use pitchforge::signals::{self, PluckParams};
use serde::{Deserialize, Serialize};

/// One simulated plucked note.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimNote {
    /// Sounding frequency in Hz.
    pub frequency_hz: f64,
    /// Duration before the next pluck, seconds.
    pub duration_s: f64,
}

/// A looping sequence of plucked notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SimulationSpec {
    /// Notes, played in order and looped.
    pub notes: Vec<SimNote>,
    /// Sample rate of the simulated stream.
    pub sample_rate: u32,
    /// Peak level, 0–1.
    pub level: f32,
    /// Background noise as signal-to-noise ratio in dB; `None` = clean.
    pub noise_snr_db: Option<f64>,
}

impl Default for SimulationSpec {
    /// A slow walk over some notes in the oud's range, including
    /// quarter-tones, at La4 = 440 Hz.
    fn default() -> Self {
        let r = ReferencePitch::default();
        let notes = [
            "Re3",
            "Mi half-flat 3",
            "Fa3",
            "Sol3",
            "La3",
            "Si half-flat 3",
            "Do4",
            "Re4",
            "Do4",
            "Si half-flat 3",
            "La3",
            "Sol3",
            "Do3",
            "Fa2",
        ]
        .iter()
        .map(|n| SimNote {
            frequency_hz: n
                .parse::<SpelledNote>()
                .expect("valid note")
                .nominal_pitch()
                .to_frequency(r),
            duration_s: 0.7,
        })
        .collect();
        Self {
            notes,
            sample_rate: 48_000,
            level: 0.5,
            noise_snr_db: Some(35.0),
        }
    }
}

impl SimulationSpec {
    /// Render one full loop of the sequence.
    pub fn render(&self) -> Vec<f32> {
        let sr = self.sample_rate as f64;
        let params = PluckParams {
            pick_noise: 0.4,
            t60: 1.5,
            ..PluckParams::default()
        };
        let mut out = Vec::new();
        for (i, n) in self.notes.iter().enumerate() {
            let len = (n.duration_s.max(0.01) * sr) as usize;
            let mut tone = signals::pluck(n.frequency_hz, sr, len, self.level, params);
            if let Some(snr) = self.noise_snr_db {
                signals::add_noise(&mut tone, snr, i as u64);
            }
            out.extend_from_slice(&tone);
        }
        if out.is_empty() {
            out = vec![0.0; self.sample_rate as usize];
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spec_renders_expected_length() {
        let spec = SimulationSpec::default();
        let audio = spec.render();
        let expected: f64 = spec.notes.iter().map(|n| n.duration_s).sum::<f64>() * 48_000.0;
        assert!((audio.len() as f64 - expected).abs() < spec.notes.len() as f64);
        assert!(audio.iter().all(|x| x.abs() <= 1.0));
    }

    #[test]
    fn empty_spec_renders_silence() {
        let spec = SimulationSpec {
            notes: vec![],
            ..SimulationSpec::default()
        };
        assert!(spec.render().iter().all(|&x| x == 0.0));
    }
}
