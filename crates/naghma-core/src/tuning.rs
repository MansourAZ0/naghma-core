//! Mapping continuous pitch to named notes.
//!
//! A [`TuningSystem`] answers one question: *which named note is this pitch
//! nearest to, and by how many cents does it deviate?* The measured pitch is
//! never modified; the deviation is always reported alongside the name.
//!
//! Two kinds of tuning system are provided:
//!
//! * [`EqualTemperament`] — `N` equal divisions of the octave. 24-EDO
//!   (50-cent quarter-tones) is the default. Names come from a
//!   [`SpellingTable`] so enharmonic choices (e.g. *Mi half-flat* vs. *Re
//!   half-sharp*… ) are configurable data, not code.
//! * [`IntonationProfile`] — arbitrary targets in cents above a tonic, for
//!   intonations that are not equal-tempered (e.g. a *Si half-flat* that
//!   is not exactly 50 cents below *Si*). Profiles carry provenance so
//!   unvalidated data is never presented as authoritative.

use crate::notation::{Accidental, Degree, SpelledNote};
use crate::pitch::Pitch;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Result of matching a pitch against a tuning system.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMatch {
    /// Name of the nearest target.
    pub note: SpelledNote,
    /// Pitch of the nearest target.
    pub target: Pitch,
    /// `pitch − target` in cents (positive = sharp).
    pub deviation_cents: f64,
}

/// Anything that can name the nearest note to a pitch.
pub trait TuningSystem {
    /// Nearest named target to `pitch`.
    fn nearest(&self, pitch: Pitch) -> NoteMatch;

    /// All named targets with pitches in `[low, high]`, ascending.
    fn targets_between(&self, low: Pitch, high: Pitch) -> Vec<NoteMatch>;
}

// ---------------------------------------------------------------------------
// Equal temperament
// ---------------------------------------------------------------------------

/// `N` equal divisions of the octave, anchored at Do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqualTemperament {
    divisions: u16,
    spelling: SpellingTable,
}

impl EqualTemperament {
    /// 24-EDO (quarter-tones) with the default spelling.
    pub fn edo24() -> Self {
        Self {
            divisions: 24,
            spelling: SpellingTable::default_24(),
        }
    }

    /// 12-EDO (semitones) with the default spelling.
    pub fn edo12() -> Self {
        Self::new(12, SpellingTable::default_24()).expect("12 divides 24")
    }

    /// `divisions`-EDO named by `spelling`. `divisions` must divide 24 so
    /// every step can be spelled with quarter-tone accidentals.
    pub fn new(divisions: u16, spelling: SpellingTable) -> Result<Self, TuningError> {
        if divisions == 0 || 24 % divisions != 0 {
            return Err(TuningError::UnsupportedDivisions(divisions));
        }
        Ok(Self {
            divisions,
            spelling,
        })
    }

    /// Steps per octave.
    pub fn divisions(&self) -> u16 {
        self.divisions
    }

    /// Size of one step in semitones.
    pub fn step_semitones(&self) -> f64 {
        12.0 / self.divisions as f64
    }

    /// The spelling table in use.
    pub fn spelling(&self) -> &SpellingTable {
        &self.spelling
    }

    /// Index of the grid step nearest `pitch` (step 0 = Do−1).
    pub fn nearest_step(&self, pitch: Pitch) -> i64 {
        (pitch.0 / self.step_semitones()).round() as i64
    }

    /// Match for a given grid step.
    fn step_match(&self, step: i64, pitch: Pitch) -> NoteMatch {
        let target = Pitch(step as f64 * self.step_semitones());
        let quarter_step = step * (24 / self.divisions as i64);
        NoteMatch {
            note: self.spelling.spell(quarter_step),
            target,
            deviation_cents: target.cents_to(pitch),
        }
    }
}

impl Default for EqualTemperament {
    fn default() -> Self {
        Self::edo24()
    }
}

impl TuningSystem for EqualTemperament {
    fn nearest(&self, pitch: Pitch) -> NoteMatch {
        self.step_match(self.nearest_step(pitch), pitch)
    }

    fn targets_between(&self, low: Pitch, high: Pitch) -> Vec<NoteMatch> {
        let s = self.step_semitones();
        let first = (low.0 / s).ceil() as i64;
        let last = (high.0 / s).floor() as i64;
        (first..=last)
            .map(|k| {
                let m = self.step_match(k, Pitch(k as f64 * s));
                NoteMatch {
                    deviation_cents: 0.0,
                    ..m
                }
            })
            .collect()
    }
}

/// Preferred spelling for each of the 24 quarter-tone steps of an octave,
/// starting at Do.
///
/// The default table ([`SpellingTable::default_24`]) is **provisional**: it
/// picks one common spelling per step, but which spelling is correct
/// depends on musical context (e.g. the maqam), so applications should let
/// users or maqam data override it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<NoteSpelling>", into = "Vec<NoteSpelling>")]
pub struct SpellingTable {
    steps: [NoteSpelling; 24],
}

/// A degree plus accidental, without octave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSpelling {
    /// Degree.
    pub degree: Degree,
    /// Accidental.
    pub accidental: Accidental,
}

impl NoteSpelling {
    /// Construct a spelling.
    pub const fn new(degree: Degree, accidental: Accidental) -> Self {
        Self { degree, accidental }
    }

    /// Pitch class in quarter-tone steps above Do, `0..24`.
    pub fn quarter_tone_class(self) -> i64 {
        (2 * self.degree.natural_semitones() as i64 + self.accidental.quarter_tones() as i64)
            .rem_euclid(24)
    }
}

impl SpellingTable {
    /// Default (provisional) quarter-tone spelling.
    ///
    /// | step | name | step | name |
    /// |---|---|---|---|
    /// | 0 | Do | 12 | Fa♯ |
    /// | 1 | Do half-sharp | 13 | Sol half-flat |
    /// | 2 | Do♯ | 14 | Sol |
    /// | 3 | Re half-flat | 15 | Sol half-sharp |
    /// | 4 | Re | 16 | La♭ |
    /// | 5 | Re half-sharp | 17 | La half-flat |
    /// | 6 | Mi♭ | 18 | La |
    /// | 7 | Mi half-flat | 19 | La half-sharp |
    /// | 8 | Mi | 20 | Si♭ |
    /// | 9 | Mi half-sharp | 21 | Si half-flat |
    /// | 10 | Fa | 22 | Si |
    /// | 11 | Fa half-sharp | 23 | Si half-sharp |
    pub fn default_24() -> Self {
        use Degree::*;
        let (n, hs, s, hf, f) = (
            Accidental::NATURAL,
            Accidental::HALF_SHARP,
            Accidental::SHARP,
            Accidental::HALF_FLAT,
            Accidental::FLAT,
        );
        let sp = NoteSpelling::new;
        Self {
            steps: [
                sp(Do, n),
                sp(Do, hs),
                sp(Do, s),
                sp(Re, hf),
                sp(Re, n),
                sp(Re, hs),
                sp(Mi, f),
                sp(Mi, hf),
                sp(Mi, n),
                sp(Mi, hs),
                sp(Fa, n),
                sp(Fa, hs),
                sp(Fa, s),
                sp(Sol, hf),
                sp(Sol, n),
                sp(Sol, hs),
                sp(La, f),
                sp(La, hf),
                sp(La, n),
                sp(La, hs),
                sp(Si, f),
                sp(Si, hf),
                sp(Si, n),
                sp(Si, hs),
            ],
        }
    }

    /// Build a table from 24 spellings; entry `k` must spell quarter-tone
    /// step `k` above Do.
    pub fn new(steps: [NoteSpelling; 24]) -> Result<Self, TuningError> {
        for (k, s) in steps.iter().enumerate() {
            if s.quarter_tone_class() != k as i64 {
                return Err(TuningError::SpellingMismatch {
                    step: k as u8,
                    spelling: *s,
                });
            }
        }
        Ok(Self { steps })
    }

    /// Replace the spelling of one step (e.g. prefer Re♭ over Do♯).
    pub fn with(mut self, spelling: NoteSpelling) -> Self {
        self.steps[spelling.quarter_tone_class() as usize] = spelling;
        self
    }

    /// Preferred spelling of quarter-tone class `k` (`0..24`).
    pub fn get(&self, k: usize) -> NoteSpelling {
        self.steps[k % 24]
    }

    /// Spell absolute quarter-tone step `q` (`q = 2 × pitch`; 120 = Do4).
    pub fn spell(&self, q: i64) -> SpelledNote {
        spell_with(self.steps[q.rem_euclid(24) as usize], q)
    }

    /// Every spelling with an accidental of at most two quarter-tones for
    /// absolute quarter-tone step `q`, preferred spelling first.
    pub fn alternatives(&self, q: i64) -> Vec<SpelledNote> {
        let preferred = self.steps[q.rem_euclid(24) as usize];
        let mut out = vec![spell_with(preferred, q)];
        for &d in &Degree::ALL {
            for a in -2..=2 {
                let s = NoteSpelling::new(d, Accidental::new(a).expect("in range"));
                if s != preferred && s.quarter_tone_class() == q.rem_euclid(24) {
                    out.push(spell_with(s, q));
                }
            }
        }
        out
    }
}

/// Spell absolute quarter-tone step `q` with a given degree/accidental
/// whose class matches. The octave is that of the degree: Do half-flat
/// at q = 119 is in octave 4.
fn spell_with(s: NoteSpelling, q: i64) -> SpelledNote {
    let letter_q = q - s.accidental.quarter_tones() as i64; // natural degree's step
    let octave = letter_q.div_euclid(24) - 1;
    SpelledNote::new(s.degree, s.accidental, octave as i8)
}

impl TryFrom<Vec<NoteSpelling>> for SpellingTable {
    type Error = TuningError;
    fn try_from(v: Vec<NoteSpelling>) -> Result<Self, Self::Error> {
        let len = v.len();
        let steps: [NoteSpelling; 24] = v
            .try_into()
            .map_err(|_| TuningError::SpellingTableLength(len))?;
        Self::new(steps)
    }
}

impl From<SpellingTable> for Vec<NoteSpelling> {
    fn from(t: SpellingTable) -> Self {
        t.steps.to_vec()
    }
}

// ---------------------------------------------------------------------------
// Custom intonation profiles
// ---------------------------------------------------------------------------

/// Where a dataset comes from and how far it can be trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// Validation status.
    pub status: ProvenanceStatus,
    /// Free-text source (author, book, recording, …).
    pub source: String,
    /// Dataset version.
    #[serde(default)]
    pub version: Option<String>,
}

/// Trust level of a dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProvenanceStatus {
    /// Supplied and validated by a qualified musician for this project.
    Validated,
    /// Plausible but not validated; UIs must label it as such.
    Provisional,
    /// Synthetic data for testing code paths only. Never musically meaningful.
    SyntheticTestData,
}

/// A target pitch inside an [`IntonationProfile`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntonationTarget {
    /// Spellings of this target, preferred first (at least one).
    pub spellings: Vec<NoteSpelling>,
    /// Position above the profile's tonic, in cents, `[0, 1200)`.
    pub cents_above_tonic: f64,
    /// Accepted deviation in cents for "in tune" judgements, if specified.
    #[serde(default)]
    pub tolerance_cents: Option<f64>,
    /// Acceptable range `[low, high]` in cents above the tonic, if the
    /// target is a region rather than a point.
    #[serde(default)]
    pub range_cents: Option<(f64, f64)>,
}

/// Arbitrary (not necessarily equal-tempered) pitch targets relative to a
/// tonic, repeating every octave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntonationProfile {
    /// Stable identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Tonic spelling.
    pub tonic: NoteSpelling,
    /// Tonic offset from its 24-EDO nominal pitch, in cents (usually 0).
    #[serde(default)]
    pub tonic_offset_cents: f64,
    /// Targets within one octave above the tonic.
    pub targets: Vec<IntonationTarget>,
    /// Associated maqam or context identifier, if any.
    #[serde(default)]
    pub context: Option<String>,
    /// Provenance; required so unvalidated data cannot pass as authoritative.
    pub provenance: Provenance,
}

impl IntonationProfile {
    /// Check internal consistency. Call after deserialising.
    pub fn validate(&self) -> Result<(), TuningError> {
        let err = |msg: String| TuningError::InvalidProfile {
            id: self.id.clone(),
            message: msg,
        };
        if self.targets.is_empty() {
            return Err(err("no targets".into()));
        }
        for (i, t) in self.targets.iter().enumerate() {
            if !(0.0..1200.0).contains(&t.cents_above_tonic) {
                return Err(err(format!(
                    "target {i}: cents_above_tonic outside [0, 1200)"
                )));
            }
            if t.spellings.is_empty() {
                return Err(err(format!("target {i}: no spelling")));
            }
            if let Some(tol) = t.tolerance_cents {
                if !(tol.is_finite() && tol >= 0.0) {
                    return Err(err(format!("target {i}: invalid tolerance")));
                }
            }
            if let Some((lo, hi)) = t.range_cents {
                if !(lo <= t.cents_above_tonic && t.cents_above_tonic <= hi) {
                    return Err(err(format!("target {i}: range does not contain target")));
                }
            }
        }
        Ok(())
    }

    fn tonic_pitch_class(&self) -> f64 {
        self.tonic.quarter_tone_class() as f64 * 0.5 + self.tonic_offset_cents / 100.0
    }

    /// Absolute pitch of target `i` in the octave whose tonic is at or
    /// below `pitch`'s octave position.
    fn target_match(&self, i: usize, octave_base: f64, pitch: Pitch) -> NoteMatch {
        let t = &self.targets[i];
        let target = Pitch(octave_base + t.cents_above_tonic / 100.0);
        let spelling = t.spellings[0];
        // The target may sit between grid steps; spell it from the nearest
        // absolute quarter-tone step that belongs to the spelling's class.
        let near = (target.0 * 2.0).round() as i64;
        let below = (near - spelling.quarter_tone_class()).rem_euclid(24);
        let q = if below <= 12 {
            near - below
        } else {
            near + (24 - below)
        };
        NoteMatch {
            note: spell_with(spelling, q),
            target,
            deviation_cents: target.cents_to(pitch),
        }
    }
}

impl TuningSystem for IntonationProfile {
    fn nearest(&self, pitch: Pitch) -> NoteMatch {
        let tonic = self.tonic_pitch_class();
        let rel = pitch.0 - tonic;
        let base = tonic + 12.0 * (rel / 12.0).floor();
        // Candidates in this octave plus the first target of the next octave
        // and the last of the previous one (to handle wrap-around).
        let mut best: Option<NoteMatch> = None;
        for octave_base in [base - 12.0, base, base + 12.0] {
            for i in 0..self.targets.len() {
                let m = self.target_match(i, octave_base, pitch);
                if best.is_none_or(|b| m.deviation_cents.abs() < b.deviation_cents.abs()) {
                    best = Some(m);
                }
            }
        }
        best.expect("validated profile has targets")
    }

    fn targets_between(&self, low: Pitch, high: Pitch) -> Vec<NoteMatch> {
        let tonic = self.tonic_pitch_class();
        let first_octave = ((low.0 - tonic) / 12.0).floor() as i64;
        let last_octave = ((high.0 - tonic) / 12.0).floor() as i64;
        let mut out = Vec::new();
        for o in first_octave..=last_octave {
            let base = tonic + 12.0 * o as f64;
            for i in 0..self.targets.len() {
                let m = self.target_match(i, base, Pitch(base));
                if m.target >= low && m.target <= high {
                    out.push(NoteMatch {
                        deviation_cents: 0.0,
                        ..m
                    });
                }
            }
        }
        out.sort_by(|a, b| a.target.0.total_cmp(&b.target.0));
        out
    }
}

/// Errors from tuning-system construction or validation.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TuningError {
    /// EDO size that does not divide 24.
    UnsupportedDivisions(u16),
    /// A spelling does not name the step it is assigned to.
    SpellingMismatch {
        /// Quarter-tone step.
        step: u8,
        /// Offending spelling.
        spelling: NoteSpelling,
    },
    /// Spelling table without exactly 24 entries.
    SpellingTableLength(usize),
    /// Inconsistent intonation profile.
    InvalidProfile {
        /// Profile id.
        id: String,
        /// Explanation.
        message: String,
    },
}

impl fmt::Display for TuningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedDivisions(n) => write!(f, "{n}-EDO is not supported (must divide 24)"),
            Self::SpellingMismatch { step, spelling } => write!(
                f,
                "spelling {:?} {:?} does not name quarter-tone step {step}",
                spelling.degree, spelling.accidental
            ),
            Self::SpellingTableLength(n) => write!(f, "spelling table needs 24 entries, got {n}"),
            Self::InvalidProfile { id, message } => {
                write!(f, "invalid intonation profile {id:?}: {message}")
            }
        }
    }
}

impl std::error::Error for TuningError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::ReferencePitch;

    fn note(s: &str) -> SpelledNote {
        s.parse().unwrap()
    }

    #[test]
    fn edo24_maps_quarter_tones_and_keeps_deviation() {
        let t = EqualTemperament::edo24();
        let r = ReferencePitch::default();
        // La4 exactly.
        let m = t.nearest(Pitch::from_frequency(440.0, r).unwrap());
        assert_eq!(m.note, note("La4"));
        assert!(m.deviation_cents.abs() < 1e-9);
        // Mi half-flat 4 = 63.5, measured 6 cents sharp.
        let m = t.nearest(Pitch(63.56));
        assert_eq!(m.note, note("Mi half-flat 4"));
        assert!((m.deviation_cents - 6.0).abs() < 1e-9);
        // 24 cents above Mi4 is still Mi4; 26 cents above is Mi half-sharp.
        assert_eq!(t.nearest(Pitch(64.24)).note, note("Mi4"));
        assert_eq!(t.nearest(Pitch(64.26)).note, note("Mi half-sharp 4"));
    }

    #[test]
    fn every_step_of_several_octaves_is_spelled_consistently() {
        let t = EqualTemperament::edo24();
        for q in (2 * 24)..(2 * 108) {
            let p = Pitch(q as f64 * 0.5);
            let m = t.nearest(p);
            assert_eq!(m.note.nominal_pitch(), p, "step {q}: {}", m.note);
        }
    }

    #[test]
    fn octave_boundaries() {
        let t = EqualTemperament::edo24();
        assert_eq!(t.nearest(Pitch(59.0)).note, note("Si3"));
        assert_eq!(t.nearest(Pitch(59.5)).note, note("Si half-sharp 3"));
        assert_eq!(t.nearest(Pitch(60.0)).note, note("Do4"));
        // With a table preferring Do half-flat, the same pitch is octave 4.
        let table =
            SpellingTable::default_24().with(NoteSpelling::new(Degree::Do, Accidental::HALF_FLAT));
        let t = EqualTemperament::new(24, table).unwrap();
        assert_eq!(t.nearest(Pitch(59.5)).note, note("Do half-flat 4"));
    }

    #[test]
    fn edo12_uses_semitones() {
        let t = EqualTemperament::edo12();
        assert_eq!(
            t.nearest(Pitch(63.5)).note.accidental.quarter_tones() % 2,
            0
        );
        let m = t.nearest(Pitch(63.4));
        assert_eq!(m.note, note("Mi flat 4"));
        assert!((m.deviation_cents - 40.0).abs() < 1e-9);
        assert!(EqualTemperament::new(7, SpellingTable::default_24()).is_err());
    }

    #[test]
    fn spelling_table_validation_and_alternatives() {
        let mut steps = [NoteSpelling::new(Degree::Do, Accidental::NATURAL); 24];
        steps.copy_from_slice(&Vec::<NoteSpelling>::from(SpellingTable::default_24()));
        steps[2] = NoteSpelling::new(Degree::Re, Accidental::FLAT);
        assert!(SpellingTable::new(steps).is_ok());
        steps[3] = NoteSpelling::new(Degree::Re, Accidental::FLAT);
        assert!(matches!(
            SpellingTable::new(steps),
            Err(TuningError::SpellingMismatch { step: 3, .. })
        ));

        let alts = SpellingTable::default_24().alternatives(2 * 63); // Mi♭4 / Re♯4
        assert_eq!(alts[0], note("Mi flat 4"));
        assert!(alts.contains(&note("Re sharp 4")));
        let alts = SpellingTable::default_24().alternatives(2 * 60 - 1); // Si half-sharp 3
        assert!(alts.contains(&note("Do half-flat 4")));

        let json = serde_json::to_string(&SpellingTable::default_24()).unwrap();
        let back: SpellingTable = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SpellingTable::default_24());
        assert!(serde_json::from_str::<SpellingTable>("[]").is_err());
    }

    #[test]
    fn targets_between_lists_grid() {
        let t = EqualTemperament::edo24();
        let names: Vec<String> = t
            .targets_between(Pitch(60.0), Pitch(62.0))
            .iter()
            .map(|m| m.note.to_string())
            .collect();
        assert_eq!(
            names,
            [
                "Do4",
                "Do half-sharp 4",
                "Do sharp 4",
                "Re half-flat 4",
                "Re4"
            ]
        );
    }

    /// SYNTHETIC TEST DATA — not a real maqam intonation.
    fn synthetic_profile() -> IntonationProfile {
        serde_json::from_str(
            r#"{
              "id": "test-profile",
              "name": "Synthetic test profile",
              "tonic": { "degree": "re", "accidental": 0 },
              "targets": [
                { "spellings": [{ "degree": "re", "accidental": 0 }], "centsAboveTonic": 0 },
                { "spellings": [{ "degree": "mi", "accidental": -1 }], "centsAboveTonic": 160, "toleranceCents": 15 },
                { "spellings": [{ "degree": "fa", "accidental": 0 }], "centsAboveTonic": 300 },
                { "spellings": [{ "degree": "sol", "accidental": 0 }], "centsAboveTonic": 500,
                  "rangeCents": [490, 510] }
              ],
              "provenance": { "status": "syntheticTestData", "source": "unit test" }
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn intonation_profile_uses_non_equal_targets() {
        let p = synthetic_profile();
        p.validate().unwrap();
        // Re4 = 62. Target "Mi half-flat" at 160 cents above → 63.6.
        let m = p.nearest(Pitch(63.5));
        assert_eq!(m.note, note("Mi half-flat 4"));
        assert!((m.deviation_cents - -10.0).abs() < 1e-9);
        // Wrap-around: just below Re5 maps to Re5, not Sol4.
        let m = p.nearest(Pitch(73.8));
        assert_eq!(m.note, note("Re5"));
        // Below the tonic in the octave: Sol3 = 55.
        assert_eq!(p.nearest(Pitch(55.1)).note, note("Sol3"));
        let between = p.targets_between(Pitch(62.0), Pitch(74.0));
        assert_eq!(between.len(), 5);
        assert_eq!(between[4].note, note("Re5"));
    }

    #[test]
    fn intonation_profile_validation() {
        let mut p = synthetic_profile();
        p.targets[1].cents_above_tonic = 1300.0;
        assert!(p.validate().is_err());
        let mut p = synthetic_profile();
        p.targets[3].range_cents = Some((510.0, 520.0));
        assert!(p.validate().is_err());
        let mut p = synthetic_profile();
        p.targets.clear();
        assert!(p.validate().is_err());
    }
}
