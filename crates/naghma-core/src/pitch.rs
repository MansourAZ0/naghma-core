//! Continuous pitch, reference tuning, intervals and instrument transposition.
//!
//! Pitches are represented on a continuous semitone scale (the MIDI note
//! number scale extended to real numbers): `69.0` is La4 at the reference
//! frequency, `60.0` is Do4, `64.5` is a quarter-tone above Mi4. Pitches are
//! never rounded here — rounding to a named note is the job of a
//! [`TuningSystem`](crate::tuning::TuningSystem), and the measured deviation
//! is always kept.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Number of cents in a semitone.
pub const CENTS_PER_SEMITONE: f64 = 100.0;

/// Continuous pitch in semitones; `69.0` = La4 at the reference frequency.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pitch(pub f64);

impl Pitch {
    /// Pitch number of La4, the usual reference note.
    pub const LA4: Pitch = Pitch(69.0);
    /// Pitch number of Do4 (scientific octave numbering: middle Do).
    pub const DO4: Pitch = Pitch(60.0);

    /// `m = 69 + 12·log2(f / f_ref)`. Returns `None` for non-positive or
    /// non-finite frequencies.
    pub fn from_frequency(hz: f64, reference: ReferencePitch) -> Option<Pitch> {
        (hz.is_finite() && hz > 0.0)
            .then(|| Pitch(Self::LA4.0 + 12.0 * (hz / reference.la4_hz).log2()))
    }

    /// Frequency in Hz under `reference`.
    pub fn to_frequency(self, reference: ReferencePitch) -> f64 {
        reference.la4_hz * 2f64.powf((self.0 - Self::LA4.0) / 12.0)
    }

    /// Semitone value.
    #[inline]
    pub fn semitones(self) -> f64 {
        self.0
    }

    /// Signed distance from `self` up to `other`, in cents.
    #[inline]
    pub fn cents_to(self, other: Pitch) -> f64 {
        (other.0 - self.0) * CENTS_PER_SEMITONE
    }

    /// Pitch moved by `interval`.
    #[inline]
    pub fn shifted(self, interval: Interval) -> Pitch {
        Pitch(self.0 + interval.semitones())
    }
}

/// Absolute reference: the frequency assigned to La4.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencePitch {
    /// Frequency of La4 in Hz (commonly 440).
    pub la4_hz: f64,
}

impl ReferencePitch {
    /// Create a reference; `None` unless `la4_hz` is a positive finite number.
    pub fn new(la4_hz: f64) -> Option<Self> {
        (la4_hz.is_finite() && la4_hz > 0.0).then_some(Self { la4_hz })
    }
}

impl Default for ReferencePitch {
    fn default() -> Self {
        Self { la4_hz: 440.0 }
    }
}

/// A signed pitch interval, stored in cents.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Interval(f64);

impl Interval {
    /// The zero interval.
    pub const UNISON: Interval = Interval(0.0);

    /// Interval of `cents` cents.
    pub const fn from_cents(cents: f64) -> Self {
        Self(cents)
    }

    /// Interval of `n` equal-tempered semitones.
    pub fn from_semitones(n: f64) -> Self {
        Self(n * CENTS_PER_SEMITONE)
    }

    /// Size in cents.
    pub const fn cents(self) -> f64 {
        self.0
    }

    /// Size in semitones.
    pub fn semitones(self) -> f64 {
        self.0 / CENTS_PER_SEMITONE
    }

    /// Frequency ratio `2^(cents/1200)`.
    pub fn ratio(self) -> f64 {
        2f64.powf(self.0 / 1200.0)
    }
}

/// Unit in which a [`Transposition`] amount is expressed.
///
/// Oud players say "−1" or "−2" for lowered tunings. Whether that counts
/// semitones or whole tones is a convention the user chooses; the unit is
/// therefore stored explicitly instead of being assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntervalUnit {
    /// 100 cents.
    #[default]
    Semitone,
    /// 200 cents.
    WholeTone,
    /// 50 cents.
    QuarterTone,
    /// 1 cent.
    Cent,
}

impl IntervalUnit {
    /// Size of one unit in cents.
    pub fn cents(self) -> f64 {
        match self {
            Self::Semitone => 100.0,
            Self::WholeTone => 200.0,
            Self::QuarterTone => 50.0,
            Self::Cent => 1.0,
        }
    }
}

/// How far an instrument sounds from the pitches its player names.
///
/// A player whose oud is tuned two semitones low uses
/// `Transposition::new(-2.0, IntervalUnit::Semitone)`. When they finger what
/// they call Re4, the instrument *sounds* Do4. The relationship is
///
/// ```text
/// m_sounding = m_player + t        m_player = m_sounding − t
/// ```
///
/// where `t` is the (signed) transposition in semitones. The measured
/// frequency is never altered — transposition only changes how a sounding
/// pitch is *named* for the player.
///
/// ```
/// use naghma_core::pitch::{IntervalUnit, Pitch, Transposition};
///
/// let t = Transposition::new(-2.0, IntervalUnit::Semitone);
/// let sounding = Pitch(60.0); // Do4 sounds…
/// assert_eq!(t.sounding_to_player(sounding), Pitch(62.0)); // …the player calls it Re4.
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transposition {
    /// Signed amount in `unit`s (negative = instrument sounds lower).
    pub amount: f64,
    /// Unit of `amount`.
    pub unit: IntervalUnit,
}

impl Transposition {
    /// No transposition.
    pub const NONE: Transposition = Transposition {
        amount: 0.0,
        unit: IntervalUnit::Semitone,
    };

    /// Transposition of `amount` `unit`s.
    pub const fn new(amount: f64, unit: IntervalUnit) -> Self {
        Self { amount, unit }
    }

    /// Transposition as an interval (sounding minus player).
    pub fn interval(&self) -> Interval {
        Interval::from_cents(self.amount * self.unit.cents())
    }

    /// `true` if this transposition is zero.
    pub fn is_none(&self) -> bool {
        self.amount == 0.0
    }

    /// Player-relative pitch for a sounding pitch: `m_player = m_sounding − t`.
    pub fn sounding_to_player(&self, sounding: Pitch) -> Pitch {
        Pitch(sounding.0 - self.interval().semitones())
    }

    /// Sounding pitch for a player-relative pitch: `m_sounding = m_player + t`.
    pub fn player_to_sounding(&self, player: Pitch) -> Pitch {
        Pitch(player.0 + self.interval().semitones())
    }
}

impl Default for Transposition {
    fn default() -> Self {
        Self::NONE
    }
}

impl fmt::Display for Transposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = match self.unit {
            IntervalUnit::Semitone => "semitone",
            IntervalUnit::WholeTone => "whole tone",
            IntervalUnit::QuarterTone => "quarter-tone",
            IntervalUnit::Cent => "cent",
        };
        let plural = if self.amount.abs() == 1.0 { "" } else { "s" };
        write!(f, "{:+} {unit}{plural}", self.amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn frequency_round_trip() {
        let r = ReferencePitch::default();
        assert!(close(
            Pitch::from_frequency(440.0, r).unwrap().0,
            69.0,
            1e-12
        ));
        assert!(close(
            Pitch::from_frequency(261.625_565, r).unwrap().0,
            60.0,
            1e-6
        ));
        for m in [24.0, 36.5, 60.0, 64.5, 81.25] {
            let p = Pitch(m);
            assert!(close(
                Pitch::from_frequency(p.to_frequency(r), r).unwrap().0,
                m,
                1e-9
            ));
        }
        assert_eq!(Pitch::from_frequency(0.0, r), None);
        assert_eq!(Pitch::from_frequency(f64::NAN, r), None);
    }

    #[test]
    fn reference_pitch_shifts_everything() {
        let r = ReferencePitch::new(442.0).unwrap();
        assert!(close(
            Pitch::from_frequency(442.0, r).unwrap().0,
            69.0,
            1e-12
        ));
        assert!(close(
            Pitch::from_frequency(440.0, r).unwrap().0,
            68.921_486,
            1e-6
        ));
        assert_eq!(ReferencePitch::new(-1.0), None);
    }

    #[test]
    fn spec_example_minus_two_semitones() {
        // Sounding Do4 (≈ 261.63 Hz) is the player's Re4 under −2.
        let t = Transposition::new(-2.0, IntervalUnit::Semitone);
        let sounding = Pitch::from_frequency(261.63, ReferencePitch::default()).unwrap();
        assert!(close(sounding.0, 60.0, 0.001));
        assert!(close(t.sounding_to_player(sounding).0, 62.0, 0.001));
    }

    #[test]
    fn transposition_round_trips_and_units() {
        for t in [
            Transposition::NONE,
            Transposition::new(-1.0, IntervalUnit::Semitone),
            Transposition::new(-2.0, IntervalUnit::Semitone),
            Transposition::new(-1.0, IntervalUnit::WholeTone),
            Transposition::new(1.0, IntervalUnit::QuarterTone),
            Transposition::new(-37.0, IntervalUnit::Cent),
        ] {
            for m in [40.0, 60.0, 64.5, 77.25] {
                let p = Pitch(m);
                assert!(close(
                    t.player_to_sounding(t.sounding_to_player(p)).0,
                    m,
                    1e-12
                ));
            }
        }
        // −1 whole tone ≡ −2 semitones.
        let a = Transposition::new(-1.0, IntervalUnit::WholeTone);
        let b = Transposition::new(-2.0, IntervalUnit::Semitone);
        assert_eq!(a.interval(), b.interval());
        // Quarter-tones survive transposition unchanged.
        let half_flat_mi = Pitch(63.5);
        assert!(close(b.sounding_to_player(half_flat_mi).0, 65.5, 1e-12));
    }

    #[test]
    fn transposition_serde_is_explicit() {
        let t = Transposition::new(-2.0, IntervalUnit::Semitone);
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#"{"amount":-2.0,"unit":"semitone"}"#);
        assert_eq!(serde_json::from_str::<Transposition>(&json).unwrap(), t);
        assert_eq!(t.to_string(), "-2 semitones");
    }
}
