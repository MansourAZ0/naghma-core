//! Instrument profiles and fretless fingerboard geometry.
//!
//! An [`InstrumentProfile`] describes the courses (string pairs) of a
//! fretless instrument such as the oud: their open notes **in the player's
//! notation**, per-course tuning adjustments, the vibrating string length
//! and the playable fingerboard length. The profile's
//! [`Transposition`] relates those player-notation pitches to what actually
//! sounds.
//!
//! ## Geometry
//!
//! For an ideal string of vibrating length `L`, the stopping position that
//! raises the open frequency `f_open` to `f_target` lies at
//!
//! ```text
//! x = L · (1 − f_open / f_target)
//! ```
//!
//! from the nut. This is a theoretical starting point: real positions vary
//! with string stiffness, action height, finger pressure and the player's
//! intonation, which is why profiles carry calibration data.
//!
//! Under a uniform transposition both `f_open` and `f_target` scale by the
//! same ratio, so a named position does not move — positions are computed
//! from player-notation pitches.

use crate::notation::SpelledNote;
use crate::pitch::{Pitch, ReferencePitch, Transposition};
use serde::{Deserialize, Serialize};
use std::fmt;

/// One course (a single string or a unison pair) of the instrument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    /// Open note in the player's notation.
    pub open: SpelledNote,
    /// Number of strings in the course.
    #[serde(default = "one")]
    pub strings: u8,
    /// Fine adjustment of the open pitch in cents (e.g. a course tuned a
    /// little sharp on purpose).
    #[serde(default)]
    pub adjustment_cents: f64,
}

fn one() -> u8 {
    1
}

impl Course {
    /// Open pitch in player notation, including the adjustment.
    pub fn open_pitch(&self) -> Pitch {
        Pitch(self.open.nominal_pitch().0 + self.adjustment_cents / 100.0)
    }
}

/// Measured corrections to the ideal-string model.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeometryCalibration {
    /// Constant offset in mm added to every computed position (e.g. to
    /// account for where the fingertip actually presses).
    pub position_offset_mm: f64,
}

/// A fretless instrument's tuning and geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentProfile {
    /// Stable identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Free-text description.
    #[serde(default)]
    pub description: Option<String>,
    /// Courses, ordered from lowest to highest as they are usually listed.
    pub courses: Vec<Course>,
    /// Effective vibrating string length `L` in millimetres.
    pub scale_length_mm: f64,
    /// Playable length of the fingerboard from the nut, in millimetres.
    pub fingerboard_length_mm: f64,
    /// Reference pitch the open notes are tuned against.
    #[serde(default)]
    pub reference: ReferencePitch,
    /// Global transposition (sounding − player).
    #[serde(default)]
    pub transposition: Transposition,
    /// Corrections to the ideal-string model.
    #[serde(default)]
    pub calibration: GeometryCalibration,
}

/// A place on the fingerboard where a pitch can be played.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FingerPosition {
    /// Index into [`InstrumentProfile::courses`].
    pub course: usize,
    /// Distance from the nut in millimetres (0 = open string).
    pub distance_mm: f64,
    /// `distance_mm / scale_length_mm`.
    pub fraction: f64,
    /// Interval above the open course in cents.
    pub cents_above_open: f64,
}

impl FingerPosition {
    /// `true` for an open (unstopped) course.
    pub fn is_open(&self) -> bool {
        self.distance_mm.abs() < 1e-9
    }
}

impl InstrumentProfile {
    /// The example 6-course oud profile from the project specification
    /// (Do2 – Fa2 – La2 – Re3 – Sol3 – Do4). Typical dimensions, not a
    /// measured instrument.
    pub fn oud_6_course_example() -> Self {
        serde_json::from_str(include_str!(
            "../data/instruments/oud-6-course-example.json"
        ))
        .expect("bundled profile is valid JSON")
    }

    /// Check that the profile is physically meaningful.
    pub fn validate(&self) -> Result<(), InstrumentError> {
        let err = |m: &str| Err(InstrumentError::Invalid(format!("{}: {m}", self.id)));
        if self.courses.is_empty() {
            return err("no courses");
        }
        if !(self.scale_length_mm.is_finite() && self.scale_length_mm > 0.0) {
            return err("scale length must be positive");
        }
        if !(self.fingerboard_length_mm.is_finite()
            && self.fingerboard_length_mm > 0.0
            && self.fingerboard_length_mm < self.scale_length_mm)
        {
            return err("fingerboard length must be in (0, scale length)");
        }
        if self.courses.iter().any(|c| c.strings == 0) {
            return err("every course needs at least one string");
        }
        if self.courses.iter().any(|c| !c.adjustment_cents.is_finite()) {
            return err("invalid course adjustment");
        }
        let corrected_end = self.fingerboard_length_mm - self.calibration.position_offset_mm;
        if !self.calibration.position_offset_mm.is_finite()
            || !(corrected_end > 0.0 && corrected_end < self.scale_length_mm)
        {
            return err(
                "calibration must leave the corrected fingerboard end within the vibrating string",
            );
        }
        Ok(())
    }

    /// Open pitch of course `i` in player notation.
    pub fn open_player_pitch(&self, course: usize) -> Pitch {
        self.courses[course].open_pitch()
    }

    /// Open pitch of course `i` as it actually sounds.
    pub fn open_sounding_pitch(&self, course: usize) -> Pitch {
        self.transposition
            .player_to_sounding(self.open_player_pitch(course))
    }

    /// Open frequency of course `i` in Hz (sounding).
    pub fn open_frequency(&self, course: usize) -> f64 {
        self.open_sounding_pitch(course)
            .to_frequency(self.reference)
    }

    /// Ideal stopping distance from the nut for a pitch `cents` above the
    /// open course: `L · (1 − 2^(−cents/1200))`, plus calibration offset for
    /// stopped notes.
    pub fn distance_for_interval(&self, cents: f64) -> f64 {
        let ideal = self.scale_length_mm * (1.0 - 2f64.powf(-cents / 1200.0));
        if cents.abs() < 1e-9 {
            0.0
        } else {
            ideal + self.calibration.position_offset_mm
        }
    }

    /// Interval in cents produced by stopping a course at `distance_mm`.
    pub fn interval_at_distance(&self, distance_mm: f64) -> f64 {
        let x = (distance_mm - self.calibration.position_offset_mm).max(0.0);
        1200.0 * (self.scale_length_mm / (self.scale_length_mm - x)).log2()
    }

    /// Player-notation pitch sounding when course `course` is stopped at
    /// `distance_mm` from the nut.
    pub fn pitch_at(&self, course: usize, distance_mm: f64) -> Pitch {
        Pitch(self.open_player_pitch(course).0 + self.interval_at_distance(distance_mm) / 100.0)
    }

    /// Every position (course and distance) where `player_pitch` can be
    /// played within the playable fingerboard, lowest course first.
    pub fn positions_for(&self, player_pitch: Pitch) -> Vec<FingerPosition> {
        (0..self.courses.len())
            .filter_map(|course| {
                let cents = self.open_player_pitch(course).cents_to(player_pitch);
                // Allow a hair below the open pitch so an open string that
                // is a fraction of a cent flat still counts.
                if cents < -0.5 {
                    return None;
                }
                let cents = cents.max(0.0);
                let distance_mm = self.distance_for_interval(cents);
                (distance_mm.is_finite()
                    && (0.0..=self.fingerboard_length_mm).contains(&distance_mm))
                .then(|| FingerPosition {
                    course,
                    distance_mm,
                    fraction: distance_mm / self.scale_length_mm,
                    cents_above_open: cents,
                })
            })
            .collect()
    }

    /// Lowest and highest player-notation pitches reachable on the
    /// fingerboard.
    pub fn player_range(&self) -> (Pitch, Pitch) {
        let top = self.interval_at_distance(self.fingerboard_length_mm) / 100.0;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for i in 0..self.courses.len() {
            let p = self.open_player_pitch(i).0;
            lo = lo.min(p);
            hi = hi.max(p + top);
        }
        (Pitch(lo), Pitch(hi))
    }
}

/// A named note at a place on the fingerboard.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappedNote {
    /// The note in the player's notation.
    pub note: SpelledNote,
    /// Player-notation pitch of the target.
    pub pitch: Pitch,
    /// Where it is played.
    pub position: FingerPosition,
}

impl InstrumentProfile {
    /// Every target of `tuning` (in player notation) that can be played on
    /// the fingerboard, with every position where it can be played, sorted
    /// by course then distance. Computed from geometry and tuning — nothing
    /// is hard-coded per note.
    pub fn fingerboard_map(&self, tuning: &impl crate::tuning::TuningSystem) -> Vec<MappedNote> {
        let (lo, hi) = self.player_range();
        let mut out: Vec<MappedNote> = tuning
            .targets_between(lo, hi)
            .into_iter()
            .flat_map(|m| {
                self.positions_for(m.target)
                    .into_iter()
                    .map(move |position| MappedNote {
                        note: m.note,
                        pitch: m.target,
                        position,
                    })
            })
            .collect();
        out.sort_by(|a, b| {
            a.position
                .course
                .cmp(&b.position.course)
                .then(a.position.distance_mm.total_cmp(&b.position.distance_mm))
        });
        out
    }
}

/// Invalid instrument data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstrumentError {
    /// Explanation of what is wrong.
    Invalid(String),
}

impl fmt::Display for InstrumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(m) => write!(f, "invalid instrument profile: {m}"),
        }
    }
}

impl std::error::Error for InstrumentError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::IntervalUnit;

    fn note(s: &str) -> SpelledNote {
        s.parse().unwrap()
    }

    #[test]
    fn example_profile_loads_and_validates() {
        let p = InstrumentProfile::oud_6_course_example();
        p.validate().unwrap();
        let opens: Vec<String> = p.courses.iter().map(|c| c.open.to_string()).collect();
        assert_eq!(opens, ["Do2", "Fa2", "La2", "Re3", "Sol3", "Do4"]);
        assert_eq!(p.courses[0].strings, 1);
        assert!((p.open_frequency(5) - 261.6256).abs() < 1e-3);
    }

    #[test]
    fn octave_is_at_half_the_string_and_fifth_at_a_third() {
        let p = InstrumentProfile::oud_6_course_example();
        assert!((p.distance_for_interval(1200.0) - 300.0).abs() < 1e-9);
        assert!((p.distance_for_interval(701.955) - 200.0).abs() < 1e-3);
        assert!((p.interval_at_distance(300.0) - 1200.0).abs() < 1e-9);
        // Round trip.
        for cents in [50.0, 150.0, 350.0, 1150.0] {
            let x = p.distance_for_interval(cents);
            assert!((p.interval_at_distance(x) - cents).abs() < 1e-9);
        }
    }

    #[test]
    fn positions_for_a_note_on_several_courses() {
        let p = InstrumentProfile::oud_6_course_example();
        // Do4: open on course 5, 5 semitones up course 4 (Sol3), 10 up
        // course 3 (Re3); La2 would need 15 semitones (> octave: off the board).
        let pos = p.positions_for(note("Do4").nominal_pitch());
        let courses: Vec<usize> = pos.iter().map(|q| q.course).collect();
        assert_eq!(courses, [3, 4, 5]);
        assert!(pos[2].is_open());
        assert!((pos[1].cents_above_open - 500.0).abs() < 1e-9);
        let expected = 600.0 * (1.0 - 2f64.powf(-5.0 / 12.0));
        assert!((pos[1].distance_mm - expected).abs() < 1e-9);
        // Below the lowest open course: nowhere.
        assert!(p.positions_for(note("Si1").nominal_pitch()).is_empty());
    }

    #[test]
    fn quarter_tone_positions_lie_between_semitones() {
        let p = InstrumentProfile::oud_6_course_example();
        let x = |s: &str| {
            p.positions_for(note(s).nominal_pitch())
                .into_iter()
                .find(|q| q.course == 3)
                .unwrap()
                .distance_mm
        };
        // On the Re3 course: Mi flat < Mi half-flat < Mi.
        assert!(x("Mi flat 3") < x("Mi half-flat 3"));
        assert!(x("Mi half-flat 3") < x("Mi3"));
    }

    #[test]
    fn transposition_moves_sound_not_positions() {
        let mut p = InstrumentProfile::oud_6_course_example();
        let before = p.positions_for(note("Mi half-flat 3").nominal_pitch());
        let f_before = p.open_frequency(3);
        p.transposition = Transposition::new(-2.0, IntervalUnit::Semitone);
        let after = p.positions_for(note("Mi half-flat 3").nominal_pitch());
        assert_eq!(before, after);
        // But the open course now sounds two semitones lower.
        let f_after = p.open_frequency(3);
        assert!((1200.0 * (f_before / f_after).log2() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn per_course_adjustment_and_calibration() {
        let mut p = InstrumentProfile::oud_6_course_example();
        p.courses[5].adjustment_cents = 10.0;
        // Do4 is now 10 cents *below* the open top course: not playable there.
        let pos = p.positions_for(note("Do4").nominal_pitch());
        assert!(pos.iter().all(|q| q.course != 5));
        p.calibration.position_offset_mm = 2.0;
        let x = p.distance_for_interval(100.0);
        assert!((p.interval_at_distance(x) - 100.0).abs() < 1e-9);
        assert_eq!(p.distance_for_interval(0.0), 0.0);
    }

    #[test]
    fn fingerboard_map_covers_each_course_in_quarter_tones() {
        let p = InstrumentProfile::oud_6_course_example();
        let map = p.fingerboard_map(&crate::tuning::EqualTemperament::edo24());
        // 300 mm of a 600 mm string = one octave = 24 quarter-tones + open.
        for (course, string) in p.courses.iter().enumerate().take(6) {
            let on_course: Vec<_> = map.iter().filter(|m| m.position.course == course).collect();
            assert_eq!(on_course.len(), 25, "course {course}");
            assert!(on_course[0].position.is_open());
            assert_eq!(on_course[0].note, string.open);
        }
        // Re4 is reachable on the Re3, Sol3 and Do4 courses.
        let re4: Vec<usize> = map
            .iter()
            .filter(|m| m.note == note("Re4"))
            .map(|m| m.position.course)
            .collect();
        assert_eq!(re4, [3, 4, 5]);
    }

    #[test]
    fn range_and_validation() {
        let p = InstrumentProfile::oud_6_course_example();
        let (lo, hi) = p.player_range();
        assert_eq!(lo, note("Do2").nominal_pitch());
        assert!((hi.0 - note("Do5").nominal_pitch().0).abs() < 1e-9);

        let mut bad = p.clone();
        bad.fingerboard_length_mm = 700.0;
        assert!(bad.validate().is_err());
        let mut bad = p.clone();
        bad.courses.clear();
        assert!(bad.validate().is_err());
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<InstrumentProfile>(&json).unwrap(), p);
    }

    #[test]
    fn corrected_positions_stay_on_the_physical_fingerboard() {
        let mut p = InstrumentProfile::oud_6_course_example();
        p.calibration.position_offset_mm = -20.0;
        p.validate().unwrap();
        // A quarter-tone above the open course would require a negative
        // stopping distance after correction. It is not a playable position.
        let target = Pitch(p.open_player_pitch(3).0 + 0.5);
        assert!(p.positions_for(target).iter().all(|q| q.course != 3));
        assert!(
            p.fingerboard_map(&crate::tuning::EqualTemperament::edo24())
                .iter()
                .all(|m| m.position.distance_mm >= 0.0 && m.position.distance_mm.is_finite())
        );
        p.fingerboard_length_mm = 590.0;
        assert!(p.validate().is_err());
        p.calibration.position_offset_mm = f64::NAN;
        assert!(p.validate().is_err());
        p.calibration.position_offset_mm = 590.0;
        assert!(p.validate().is_err());
    }
}
