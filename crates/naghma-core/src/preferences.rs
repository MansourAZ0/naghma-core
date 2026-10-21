//! A musician's notation preferences, shared by applications.
//!
//! These are the settings every naghma-based app exposes in the same way:
//! reference pitch, instrument transposition, tuning grid, script and
//! accidental style for note names, and whether notes are shown in the
//! player's notation or at concert pitch. Apps embed this struct in their
//! own settings (e.g. with `#[serde(flatten)]`).

use crate::interpret::{NoteView, PitchContext, Tuning};
use crate::notation::{AccidentalStyle, Digits, LabelOptions, Script};
use crate::pitch::{ReferencePitch, Transposition};
use crate::tuning::EqualTemperament;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Equal-tempered grids offered to users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TuningChoice {
    /// 24 equal divisions (quarter-tones).
    #[default]
    Edo24,
    /// 12 equal divisions (semitones).
    Edo12,
}

/// Notation preferences. Every field has a default so older stored
/// settings keep loading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotationPreferences {
    /// Frequency of La4 in Hz.
    pub reference_hz: f64,
    /// Instrument transposition (sounding − player).
    pub transposition: Transposition,
    /// Tuning grid used to name pitches.
    pub tuning: TuningChoice,
    /// Script for note names.
    pub note_script: Script,
    /// Accidentals as symbols or words.
    pub accidental_style: AccidentalStyle,
    /// Arabic-Indic digits for octave numbers.
    pub arabic_digits: bool,
    /// Show player names (default) or concert pitch.
    pub note_view: NoteView,
}

impl Default for NotationPreferences {
    fn default() -> Self {
        Self {
            reference_hz: 440.0,
            transposition: Transposition::NONE,
            tuning: TuningChoice::Edo24,
            note_script: Script::Latin,
            accidental_style: AccidentalStyle::Symbol,
            arabic_digits: false,
            note_view: NoteView::Player,
        }
    }
}

/// A preference value outside its allowed range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidPreference(pub String);

impl fmt::Display for InvalidPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidPreference {}

impl NotationPreferences {
    /// Allowed reference range in Hz.
    pub const REFERENCE_RANGE: (f64, f64) = (380.0, 480.0);

    /// Reject values that would break naming.
    pub fn validate(&self) -> Result<(), InvalidPreference> {
        let (lo, hi) = Self::REFERENCE_RANGE;
        if !(self.reference_hz.is_finite() && (lo..=hi).contains(&self.reference_hz)) {
            return Err(InvalidPreference(format!(
                "reference pitch must be between {lo} and {hi} Hz"
            )));
        }
        let t = self.transposition.interval().cents();
        if !(t.is_finite() && t.abs() <= 2400.0) {
            return Err(InvalidPreference(
                "transposition must be within two octaves".into(),
            ));
        }
        Ok(())
    }

    /// Reference pitch (falls back to 440 Hz if invalid).
    pub fn reference(&self) -> ReferencePitch {
        ReferencePitch::new(self.reference_hz).unwrap_or_default()
    }

    /// Naming context for analysis.
    pub fn pitch_context(&self) -> PitchContext {
        PitchContext {
            reference: self.reference(),
            transposition: self.transposition,
            tuning: Tuning::Equal(match self.tuning {
                TuningChoice::Edo24 => EqualTemperament::edo24(),
                TuningChoice::Edo12 => EqualTemperament::edo12(),
            }),
        }
    }

    /// Label rendering options (octave shown).
    pub fn label_options(&self) -> LabelOptions {
        LabelOptions {
            script: self.note_script,
            accidentals: self.accidental_style,
            show_octave: true,
            digits: if self.arabic_digits {
                Digits::ArabicIndic
            } else {
                Digits::Western
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::IntervalUnit;

    #[test]
    fn defaults_round_trip_and_tolerate_missing_fields() {
        let p = NotationPreferences::default();
        p.validate().unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(
            serde_json::from_str::<NotationPreferences>(&json).unwrap(),
            p
        );
        let partial: NotationPreferences =
            serde_json::from_str(r#"{"transposition":{"amount":-2,"unit":"semitone"}}"#).unwrap();
        assert_eq!(partial.reference_hz, 440.0);
        assert_eq!(partial.tuning, TuningChoice::Edo24);
    }

    #[test]
    fn validation() {
        let bad = NotationPreferences {
            reference_hz: 1000.0,
            ..NotationPreferences::default()
        };
        assert!(bad.validate().is_err());
        let bad = NotationPreferences {
            transposition: Transposition::new(-30.0, IntervalUnit::Semitone),
            ..NotationPreferences::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn context_and_labels() {
        let p = NotationPreferences {
            transposition: Transposition::new(-2.0, IntervalUnit::Semitone),
            note_script: Script::Arabic,
            arabic_digits: true,
            ..NotationPreferences::default()
        };
        let r = p.pitch_context().read(261.63).unwrap();
        assert_eq!(r.player.note.label(&p.label_options()).text, "ري٤");
        let p12 = NotationPreferences {
            tuning: TuningChoice::Edo12,
            ..NotationPreferences::default()
        };
        let r = p12.pitch_context().read(311.13 * 2f64.powf(0.24)).unwrap();
        assert_eq!(r.player.note.accidental.quarter_tones() % 2, 0);
    }
}
