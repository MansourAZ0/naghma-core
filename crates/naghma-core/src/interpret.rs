//! From a measured frequency to the names a musician sees.
//!
//! Every reading carries two identities, which are kept separate:
//!
//! * **sounding** — the note that physically sounds (concert pitch);
//! * **player** — the note the musician considers themselves to be playing,
//!   given their instrument's [`Transposition`].
//!
//! The measured frequency is stored unchanged in both cases.

use crate::pitch::{Pitch, ReferencePitch, Transposition};
use crate::tuning::{EqualTemperament, IntonationProfile, NoteMatch, TuningError, TuningSystem};
use serde::{Deserialize, Serialize};

/// Which identity a view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteView {
    /// The player's familiar names (default).
    #[default]
    Player,
    /// Concert (sounding) pitch.
    Concert,
}

/// A tuning system selectable in settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Tuning {
    /// Equal division of the octave (24-EDO by default).
    Equal(EqualTemperament),
    /// Custom intonation targets.
    Profile(IntonationProfile),
}

impl Default for Tuning {
    fn default() -> Self {
        Self::Equal(EqualTemperament::edo24())
    }
}

impl Tuning {
    /// Validate user-supplied data.
    pub fn validate(&self) -> Result<(), TuningError> {
        match self {
            Self::Equal(_) => Ok(()),
            Self::Profile(p) => p.validate(),
        }
    }
}

impl TuningSystem for Tuning {
    fn nearest(&self, pitch: Pitch) -> NoteMatch {
        match self {
            Self::Equal(t) => t.nearest(pitch),
            Self::Profile(t) => t.nearest(pitch),
        }
    }

    fn targets_between(&self, low: Pitch, high: Pitch) -> Vec<NoteMatch> {
        match self {
            Self::Equal(t) => t.targets_between(low, high),
            Self::Profile(t) => t.targets_between(low, high),
        }
    }
}

/// Everything needed to name a frequency.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PitchContext {
    /// Frequency of La4.
    pub reference: ReferencePitch,
    /// Instrument transposition (sounding − player).
    pub transposition: Transposition,
    /// Tuning system used to name pitches (in the space being named).
    pub tuning: Tuning,
}

/// A measured frequency interpreted in a [`PitchContext`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteReading {
    /// Measured frequency, unmodified.
    pub frequency_hz: f64,
    /// Continuous sounding pitch.
    pub sounding_pitch: Pitch,
    /// Continuous player-relative pitch.
    pub player_pitch: Pitch,
    /// Nearest named note at sounding (concert) pitch.
    pub sounding: NoteMatch,
    /// Nearest named note in the player's notation.
    pub player: NoteMatch,
}

impl NoteReading {
    /// The identity shown in `view`.
    pub fn view(&self, view: NoteView) -> &NoteMatch {
        match view {
            NoteView::Player => &self.player,
            NoteView::Concert => &self.sounding,
        }
    }
}

impl PitchContext {
    /// Interpret a measured frequency. `None` for non-positive or non-finite
    /// input.
    pub fn read(&self, frequency_hz: f64) -> Option<NoteReading> {
        let sounding_pitch = Pitch::from_frequency(frequency_hz, self.reference)?;
        Some(self.read_pitch(frequency_hz, sounding_pitch))
    }

    fn read_pitch(&self, frequency_hz: f64, sounding_pitch: Pitch) -> NoteReading {
        let player_pitch = self.transposition.sounding_to_player(sounding_pitch);
        NoteReading {
            frequency_hz,
            sounding_pitch,
            player_pitch,
            sounding: self.tuning.nearest(sounding_pitch),
            player: self.tuning.nearest(player_pitch),
        }
    }

    /// Frequency that sounds when the player plays `player_pitch`.
    pub fn sounding_frequency(&self, player_pitch: Pitch) -> f64 {
        self.transposition
            .player_to_sounding(player_pitch)
            .to_frequency(self.reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::SpelledNote;
    use crate::pitch::IntervalUnit;

    fn note(s: &str) -> SpelledNote {
        s.parse().unwrap()
    }

    fn ctx(semitones: f64) -> PitchContext {
        PitchContext {
            transposition: Transposition::new(semitones, IntervalUnit::Semitone),
            ..PitchContext::default()
        }
    }

    #[test]
    fn normal_tuning_player_equals_sounding() {
        let r = ctx(0.0).read(440.0).unwrap();
        assert_eq!(r.player.note, note("La4"));
        assert_eq!(r.sounding.note, note("La4"));
    }

    #[test]
    fn spec_do4_sounding_is_re4_for_player_at_minus_two() {
        let r = ctx(-2.0).read(261.63).unwrap();
        assert_eq!(r.sounding.note, note("Do4"));
        assert_eq!(r.player.note, note("Re4"));
        assert_eq!(r.view(NoteView::Player).note, note("Re4"));
        assert_eq!(r.view(NoteView::Concert).note, note("Do4"));
        // The frequency is never altered.
        assert_eq!(r.frequency_hz, 261.63);
        // Deviation is identical in both spaces.
        assert!((r.player.deviation_cents - r.sounding.deviation_cents).abs() < 1e-9);
    }

    #[test]
    fn minus_one_shifts_by_a_semitone() {
        let r = ctx(-1.0).read(440.0).unwrap();
        assert_eq!(r.sounding.note, note("La4"));
        assert_eq!(r.player.note, note("Si flat 4"));
    }

    #[test]
    fn equivalent_fingering_keeps_player_name_under_uniform_transposition() {
        // The player fingers Mi half-flat 4. Whatever the transposition, the
        // sounding frequency changes but the player name does not.
        let fingered = note("Mi half-flat 4").nominal_pitch();
        for t in [0.0, -1.0, -2.0, -3.5] {
            let c = ctx(t);
            let hz = c.sounding_frequency(fingered);
            let r = c.read(hz).unwrap();
            assert_eq!(r.player.note, note("Mi half-flat 4"), "t = {t}");
            assert!(r.player.deviation_cents.abs() < 1e-6);
        }
    }

    #[test]
    fn quarter_tones_and_octave_transitions_under_transposition() {
        let c = ctx(-2.0);
        // Sounding Si half-flat 3 (58.5) → player Do half-sharp 4 (60.5).
        let hz = Pitch(58.5).to_frequency(ReferencePitch::default());
        let r = c.read(hz).unwrap();
        assert_eq!(r.sounding.note, note("Si half-flat 3"));
        assert_eq!(r.player.note, note("Do half-sharp 4"));
        // Sounding Si3 (59) → player Do sharp 4 (61): crosses the octave.
        let hz = Pitch(59.0).to_frequency(ReferencePitch::default());
        let r = c.read(hz).unwrap();
        assert_eq!(r.sounding.note.octave, 3);
        assert_eq!(r.player.note, note("Do sharp 4"));
    }

    #[test]
    fn whole_tone_unit_convention() {
        let c = PitchContext {
            transposition: Transposition::new(-1.0, IntervalUnit::WholeTone),
            ..PitchContext::default()
        };
        assert_eq!(c.read(261.63).unwrap().player.note, note("Re4"));
    }

    #[test]
    fn rejects_invalid_frequency_and_round_trips_settings() {
        assert!(ctx(0.0).read(0.0).is_none());
        let c = ctx(-2.0);
        let json = serde_json::to_string(&c).unwrap();
        let back: PitchContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
        // Missing fields fall back to defaults (forward-compatible settings).
        let partial: PitchContext =
            serde_json::from_str(r#"{"reference":{"la4Hz":442.0}}"#).unwrap();
        assert_eq!(partial.reference.la4_hz, 442.0);
        assert_eq!(partial.transposition, Transposition::NONE);
    }
}
