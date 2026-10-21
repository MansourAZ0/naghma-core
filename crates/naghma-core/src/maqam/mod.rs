//! Data structures for maqamat and ajnas.
//!
//! **This module contains no musical definitions.** Maqam and jins data is
//! supplied by the user as JSON ([`MaqamDataset`]) and validated on load.
//! Every dataset carries [`Provenance`]; applications must label anything
//! that is not [`ProvenanceStatus::Validated`] accordingly and must not
//! present synthetic test data as music theory.
//!
//! ## Model
//!
//! * A [`Jins`] is a reusable fragment defined relative to its root: a list
//!   of [`ScaleDegree`]s, each with a *step* (how many letter names above
//!   the root — this fixes the spelling) and a size in *cents above the
//!   root* (this fixes the intonation).
//! * A [`Maqam`] has a default tonic, an ascending scale (and optionally a
//!   distinct descending one) as scale degrees relative to the tonic, and
//!   references ajnas by id with the step they sit on, so jins definitions
//!   are not duplicated.
//! * Absolute instrument tuning never appears here; [`Maqam::realize`]
//!   places a scale on a tonic to obtain spelled notes and pitches.

pub mod analysis;

use crate::notation::{Accidental, Degree, SpelledNote};
use crate::pitch::Pitch;
use crate::tuning::{NoteSpelling, Provenance};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Current dataset schema version understood by this crate.
pub const SCHEMA_VERSION: u32 = 1;

/// One note of a scale or jins, relative to its root/tonic.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleDegree {
    /// Letter-name steps above the root (0 = root, 1 = next letter, …;
    /// 7 = octave). Determines spelling.
    pub step: u8,
    /// Pitch above the root in cents. Determines intonation; may differ
    /// from 24-EDO.
    pub cents: f64,
}

/// Text in Arabic plus an optional transliteration / English rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Name {
    /// Name in Arabic script.
    pub arabic: String,
    /// Transliterated or English name.
    pub latin: String,
}

/// A jins (tetrachord, pentachord, trichord, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Jins {
    /// Stable identifier.
    pub id: String,
    /// Name.
    pub name: Name,
    /// Notes relative to the root, ascending, starting with `{step: 0, cents: 0}`.
    pub degrees: Vec<ScaleDegree>,
    /// Optional educational text.
    #[serde(default)]
    pub description: Option<String>,
}

/// Role of a jins inside a maqam.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JinsRole {
    /// The lower (root) jins.
    Lower,
    /// The upper jins.
    Upper,
    /// Any other role, described in text.
    Other(String),
}

/// A jins placed on a step of a maqam's scale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinsPlacement {
    /// Id of a [`Jins`] in the same dataset.
    pub jins: String,
    /// Scale step (letter steps above the tonic) the jins starts on.
    pub on_step: u8,
    /// Role in the maqam.
    pub role: JinsRole,
}

/// A characteristic melodic phrase, as scale steps relative to the tonic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Phrase {
    /// Steps (may be negative = below the tonic).
    pub steps: Vec<i8>,
    /// Optional explanation.
    #[serde(default)]
    pub description: Option<String>,
}

/// A maqam definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Maqam {
    /// Stable identifier.
    pub id: String,
    /// Name.
    pub name: Name,
    /// Family / parent maqam id, if any.
    #[serde(default)]
    pub family: Option<String>,
    /// Related maqam ids.
    #[serde(default)]
    pub related: Vec<String>,
    /// Usual tonic.
    pub tonic: NoteSpelling,
    /// Ascending scale, starting with `{step: 0, cents: 0}`.
    pub ascending: Vec<ScaleDegree>,
    /// Descending scale if it differs from the ascending one (listed
    /// descending, from the top).
    #[serde(default)]
    pub descending: Option<Vec<ScaleDegree>>,
    /// Constituent ajnas.
    #[serde(default)]
    pub ajnas: Vec<JinsPlacement>,
    /// Important resting tones / ghammaz, as steps above the tonic.
    #[serde(default)]
    pub resting_steps: Vec<u8>,
    /// Characteristic phrases.
    #[serde(default)]
    pub phrases: Vec<Phrase>,
    /// Ids of maqamat this one commonly modulates to.
    #[serde(default)]
    pub modulations: Vec<String>,
    /// Id of an [`IntonationProfile`](crate::tuning::IntonationProfile)
    /// giving performance intonation, if any.
    #[serde(default)]
    pub intonation_profile: Option<String>,
    /// Optional educational text.
    #[serde(default)]
    pub description: Option<String>,
}

/// A note of a maqam or jins realised on a concrete tonic.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealizedNote {
    /// Spelled note (octave included).
    pub note: SpelledNote,
    /// Target pitch from the definition's cents (not snapped to a grid).
    pub pitch: Pitch,
    /// Step above the tonic.
    pub step: u8,
}

/// Place `degrees` on `tonic`, spelling each degree with the letter `step`
/// letters above the tonic and the nearest quarter-tone accidental.
pub fn realize(tonic: SpelledNote, degrees: &[ScaleDegree]) -> Vec<RealizedNote> {
    let tonic_pitch = tonic.nominal_pitch();
    degrees
        .iter()
        .map(|d| {
            let letter_index = tonic.degree.index() as i32 + d.step as i32;
            let degree = Degree::from_index(letter_index);
            let octave = tonic.octave as i32 + letter_index.div_euclid(7);
            let natural = SpelledNote::natural(degree, octave as i8).nominal_pitch();
            let pitch = Pitch(tonic_pitch.0 + d.cents / 100.0);
            let quarter_tones = ((pitch.0 - natural.0) * 2.0).round() as i8;
            let accidental = Accidental::new(quarter_tones.clamp(-4, 4)).expect("clamped");
            RealizedNote {
                note: SpelledNote::new(degree, accidental, octave as i8),
                pitch,
                step: d.step,
            }
        })
        .collect()
}

impl Maqam {
    /// Ascending scale realised on `tonic` (see [`Maqam::default_tonic`]).
    pub fn realize(&self, tonic: SpelledNote) -> Vec<RealizedNote> {
        realize(tonic, &self.ascending)
    }

    /// Default tonic in the given octave.
    pub fn default_tonic(&self, octave: i8) -> SpelledNote {
        SpelledNote::new(self.tonic.degree, self.tonic.accidental, octave)
    }
}

/// A complete, versioned dataset of ajnas, maqamat and their provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaqamDataset {
    /// Schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Where the data comes from and how much it can be trusted.
    pub provenance: Provenance,
    /// Reusable ajnas.
    #[serde(default)]
    pub ajnas: Vec<Jins>,
    /// Maqamat.
    #[serde(default)]
    pub maqamat: Vec<Maqam>,
}

impl MaqamDataset {
    /// Parse and validate JSON.
    pub fn from_json(json: &str) -> Result<Self, MaqamDataError> {
        let ds: Self =
            serde_json::from_str(json).map_err(|e| MaqamDataError::Parse(e.to_string()))?;
        ds.validate()?;
        Ok(ds)
    }

    /// Look up a jins by id.
    pub fn jins(&self, id: &str) -> Option<&Jins> {
        self.ajnas.iter().find(|j| j.id == id)
    }

    /// Look up a maqam by id.
    pub fn maqam(&self, id: &str) -> Option<&Maqam> {
        self.maqamat.iter().find(|m| m.id == id)
    }

    /// Check schema version, unique ids, references and scale shapes.
    /// Returns every problem found, not just the first.
    pub fn validate(&self) -> Result<(), MaqamDataError> {
        let mut problems = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            problems.push(format!(
                "schema version {} is not supported (expected {SCHEMA_VERSION})",
                self.schema_version
            ));
        }
        let mut seen = HashSet::new();
        for j in &self.ajnas {
            if !seen.insert(("jins", j.id.as_str())) {
                problems.push(format!("duplicate jins id {:?}", j.id));
            }
            check_degrees(&format!("jins {:?}", j.id), &j.degrees, true, &mut problems);
        }
        let jins_ids: HashMap<&str, &Jins> =
            self.ajnas.iter().map(|j| (j.id.as_str(), j)).collect();
        let maqam_ids: HashSet<&str> = self.maqamat.iter().map(|m| m.id.as_str()).collect();
        for m in &self.maqamat {
            let ctx = format!("maqam {:?}", m.id);
            if !seen.insert(("maqam", m.id.as_str())) {
                problems.push(format!("duplicate maqam id {:?}", m.id));
            }
            check_degrees(&ctx, &m.ascending, true, &mut problems);
            if let Some(desc) = &m.descending {
                let mut asc: Vec<ScaleDegree> = desc.clone();
                asc.reverse();
                check_degrees(&format!("{ctx} descending"), &asc, false, &mut problems);
            }
            let max_step = m.ascending.iter().map(|d| d.step).max().unwrap_or(0);
            for p in &m.ajnas {
                match jins_ids.get(p.jins.as_str()) {
                    None => problems.push(format!("{ctx}: unknown jins {:?}", p.jins)),
                    Some(_) if p.on_step > max_step => {
                        problems.push(format!("{ctx}: jins {:?} placed above the scale", p.jins))
                    }
                    Some(_) => {}
                }
            }
            for r in &m.resting_steps {
                if !m.ascending.iter().any(|d| d.step == *r) {
                    problems.push(format!("{ctx}: resting step {r} is not in the scale"));
                }
            }
            for id in m
                .related
                .iter()
                .chain(&m.modulations)
                .chain(m.family.iter())
            {
                if !maqam_ids.contains(id.as_str()) {
                    problems.push(format!("{ctx}: unknown maqam reference {id:?}"));
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(MaqamDataError::Invalid(problems))
        }
    }
}

fn check_degrees(ctx: &str, degrees: &[ScaleDegree], from_root: bool, problems: &mut Vec<String>) {
    if degrees.len() < 2 {
        problems.push(format!("{ctx}: needs at least two degrees"));
        return;
    }
    if from_root && (degrees[0].step != 0 || degrees[0].cents != 0.0) {
        problems.push(format!("{ctx}: must start at step 0, 0 cents"));
    }
    for w in degrees.windows(2) {
        if w[1].step <= w[0].step || w[1].cents <= w[0].cents {
            problems.push(format!(
                "{ctx}: degrees must strictly ascend in step and cents"
            ));
            break;
        }
    }
    for d in degrees {
        if !d.cents.is_finite() || d.cents < 0.0 || d.cents > 2400.0 {
            problems.push(format!("{ctx}: cents value {} out of range", d.cents));
        }
        // The spelled letter must be within a whole tone of the pitch, or
        // the realised accidental would exceed ±4 quarter-tones.
        let natural = [0.0, 200.0, 400.0, 500.0, 700.0, 900.0, 1100.0][(d.step % 7) as usize]
            + 1200.0 * (d.step / 7) as f64;
        if (d.cents - natural).abs() > 200.0 + 1e-9 {
            problems.push(format!(
                "{ctx}: step {} at {} cents cannot be spelled on its letter",
                d.step, d.cents
            ));
        }
    }
}

/// Errors loading a [`MaqamDataset`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MaqamDataError {
    /// Not valid JSON for the schema.
    Parse(String),
    /// Structurally valid but inconsistent; one message per problem.
    Invalid(Vec<String>),
}

impl fmt::Display for MaqamDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "cannot parse maqam dataset: {e}"),
            Self::Invalid(ps) => write!(f, "invalid maqam dataset: {}", ps.join("; ")),
        }
    }
}

impl std::error::Error for MaqamDataError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::ProvenanceStatus;

    const FIXTURE: &str = include_str!("../../tests/fixtures/synthetic-maqam-dataset.json");

    fn note(s: &str) -> SpelledNote {
        s.parse().unwrap()
    }

    #[test]
    fn synthetic_fixture_loads_and_is_marked_as_test_data() {
        let ds = MaqamDataset::from_json(FIXTURE).unwrap();
        assert_eq!(ds.provenance.status, ProvenanceStatus::SyntheticTestData);
        assert_eq!(ds.maqamat.len(), 2);
        assert!(ds.jins("test-jins-x").is_some());
    }

    #[test]
    fn realize_spells_by_letter_steps_with_quarter_tones() {
        let ds = MaqamDataset::from_json(FIXTURE).unwrap();
        let m = ds.maqam("test-maqam-a").unwrap();
        let names: Vec<String> = m
            .realize(note("Do4"))
            .iter()
            .map(|r| r.note.to_string())
            .collect();
        // 0, 200, 350, 500, 700, 900, 1050, 1200 cents on Do.
        assert_eq!(
            names,
            [
                "Do4",
                "Re4",
                "Mi half-flat 4",
                "Fa4",
                "Sol4",
                "La4",
                "Si half-flat 4",
                "Do5"
            ]
        );
        // Same intervals on Sol: the letter steps keep the spelling sensible.
        let names: Vec<String> = m
            .realize(note("Sol3"))
            .iter()
            .map(|r| r.note.to_string())
            .collect();
        assert_eq!(
            names,
            [
                "Sol3",
                "La3",
                "Si half-flat 3",
                "Do4",
                "Re4",
                "Mi4",
                "Fa half-sharp 4",
                "Sol4"
            ]
        );
    }

    #[test]
    fn realize_keeps_non_edo_cents() {
        let degrees = [
            ScaleDegree {
                step: 0,
                cents: 0.0,
            },
            ScaleDegree {
                step: 2,
                cents: 340.0,
            },
        ];
        let r = realize(note("Re4"), &degrees);
        assert_eq!(r[1].note, note("Fa half-sharp 4") /* 340 c ≈ 350 */);
        assert!((r[1].pitch.0 - 65.4).abs() < 1e-9);
    }

    #[test]
    fn validation_reports_all_problems() {
        let mut ds = MaqamDataset::from_json(FIXTURE).unwrap();
        ds.schema_version = 99;
        ds.maqamat[0].ajnas[0].jins = "missing".into();
        ds.maqamat[0].related.push("nowhere".into());
        ds.maqamat[1].ascending[1].cents = 5000.0;
        let Err(MaqamDataError::Invalid(problems)) = ds.validate() else {
            panic!("expected invalid");
        };
        assert!(problems.len() >= 4, "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("schema version")));
        assert!(problems.iter().any(|p| p.contains("unknown jins")));
        assert!(problems.iter().any(|p| p.contains("nowhere")));
    }

    #[test]
    fn rejects_unspellable_and_non_ascending_degrees() {
        let bad = r#"{
          "schemaVersion": 1,
          "provenance": { "status": "syntheticTestData", "source": "unit test" },
          "ajnas": [{ "id": "j", "name": { "arabic": "اختبار", "latin": "TEST" },
                      "degrees": [{ "step": 0, "cents": 0 }, { "step": 1, "cents": 450 }] }]
        }"#;
        assert!(matches!(
            MaqamDataset::from_json(bad),
            Err(MaqamDataError::Invalid(_))
        ));
        let bad = bad.replace("450", "-10");
        assert!(MaqamDataset::from_json(&bad).is_err());
        assert!(matches!(
            MaqamDataset::from_json("{"),
            Err(MaqamDataError::Parse(_))
        ));
    }
}
