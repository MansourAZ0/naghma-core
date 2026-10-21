//! Rule-based maqam candidate scoring over a rolling note history.
//!
//! **The output is a heuristic ranking, not a probability.** Scores combine
//! configurable evidence terms; they say how well the recent notes fit each
//! maqam *as defined in the loaded dataset*, nothing more. With synthetic
//! test data the results are meaningless musically.
//!
//! ## Evidence considered
//!
//! For every maqam in the dataset and every candidate tonic:
//!
//! * **fit** — share of played time (duration-weighted) on notes that match a
//!   scale degree within the tolerance;
//! * **foreign notes** — share of played time on notes outside the scale
//!   (reported as contradicting evidence);
//! * **coverage** — share of the scale's degrees actually heard;
//! * **tonic** — emphasis on the tonic (time spent on it, and whether the
//!   most recent long note rests on it);
//! * **resting tones** — time spent on the dataset's resting steps;
//! * **ajnas** — completeness of the maqam's ajnas in what was heard;
//! * **phrases** — occurrences of the dataset's characteristic phrases in the
//!   melodic sequence.
//!
//! Different maqamat can share most of their notes; when the best scores are
//! close the result is [`AnalysisStatus::Ambiguous`] rather than a forced
//! choice, and too little material gives
//! [`AnalysisStatus::InsufficientData`].

use super::{Maqam, MaqamDataset, ScaleDegree};
use crate::events::MusicalNoteEvent;
use crate::tuning::NoteSpelling;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Which pitch identity the analysis uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PitchSpace {
    /// The player's notation (default; follows instrument transposition).
    #[default]
    Player,
    /// Sounding (concert) pitch.
    Sounding,
}

/// Weights of the evidence terms. All are configurable so the scoring can
/// be tuned and tested; none is claimed to be musically calibrated.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScoreWeights {
    /// Weight of in-scale time share.
    pub fit: f64,
    /// Penalty per unit share of out-of-scale time.
    pub foreign: f64,
    /// Weight of scale-degree coverage.
    pub coverage: f64,
    /// Weight of tonic emphasis.
    pub tonic: f64,
    /// Weight of resting-tone emphasis.
    pub resting: f64,
    /// Weight of jins completeness.
    pub ajnas: f64,
    /// Bonus per matched characteristic phrase (capped at 3).
    pub phrase: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            fit: 1.0,
            foreign: 1.5,
            coverage: 0.5,
            tonic: 0.4,
            resting: 0.2,
            ajnas: 0.3,
            phrase: 0.15,
        }
    }
}

/// Parameters of [`MaqamAnalyzer`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyzerConfig {
    /// Length of the rolling history in seconds.
    pub window_s: f64,
    /// Minimum notes in the window before any ranking is reported.
    pub min_notes: usize,
    /// Minimum distinct pitch classes before any ranking is reported.
    pub min_distinct: usize,
    /// How far (cents) a played pitch may be from a scale degree and still
    /// count as that degree.
    pub tolerance_cents: f64,
    /// Candidates whose score is within this margin of the best make the
    /// result ambiguous.
    pub ambiguity_margin: f64,
    /// Best scores below this give [`AnalysisStatus::Unknown`].
    pub min_score: f64,
    /// Maximum number of candidates returned.
    pub max_candidates: usize,
    /// Pitch identity to analyse.
    pub pitch_space: PitchSpace,
    /// Evidence weights.
    pub weights: ScoreWeights,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            window_s: 25.0,
            min_notes: 6,
            min_distinct: 4,
            tolerance_cents: 30.0,
            ambiguity_margin: 0.08,
            min_score: 0.6,
            max_candidates: 5,
            pitch_space: PitchSpace::Player,
            weights: ScoreWeights::default(),
        }
    }
}

/// One note in the analysis history.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryNote {
    /// Start time, seconds (stream clock).
    pub start_s: f64,
    /// Duration, seconds.
    pub duration_s: f64,
    /// Pitch in semitones in the chosen pitch space (60 = Do4).
    pub pitch: f64,
}

/// Overall outcome of an analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisStatus {
    /// Not enough notes or distinct pitches yet.
    InsufficientData,
    /// No dataset loaded, or no maqam fits well enough.
    Unknown,
    /// Several maqamat fit about equally well.
    Ambiguous,
    /// One candidate clearly fits best (still a heuristic judgement).
    Likely,
}

/// A piece of evidence for or against a candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Evidence {
    /// Share of played time on scale notes.
    ScaleFit {
        /// Share in `[0, 1]`.
        share: f64,
    },
    /// Played notes not in the scale.
    ForeignNotes {
        /// Share of played time in `[0, 1]`.
        share: f64,
        /// The foreign pitches (semitones, rounded to quarter-tones).
        pitches: Vec<f64>,
    },
    /// Scale degrees heard / total.
    Coverage {
        /// Degrees heard.
        heard: usize,
        /// Degrees in the scale (excluding the octave).
        total: usize,
    },
    /// Emphasis on the tonic.
    Tonic {
        /// Share of played time on the tonic.
        share: f64,
        /// The latest note rests on the tonic.
        final_rest: bool,
    },
    /// Time on resting tones.
    RestingTones {
        /// Share of played time.
        share: f64,
    },
    /// Jins completeness.
    Jins {
        /// Jins id.
        jins: String,
        /// Fraction of its notes heard.
        completeness: f64,
    },
    /// Characteristic phrase found.
    Phrase {
        /// Index of the phrase in the maqam definition.
        index: usize,
    },
}

/// A ranked maqam hypothesis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaqamCandidate {
    /// Maqam id in the dataset.
    pub maqam: String,
    /// Tonic assumed for this candidate.
    pub tonic: NoteSpelling,
    /// Heuristic score (higher fits better; not a probability).
    pub score: f64,
    /// Supporting evidence.
    pub supporting: Vec<Evidence>,
    /// Contradicting evidence.
    pub contradicting: Vec<Evidence>,
}

/// Result of [`MaqamAnalyzer::analyze`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaqamAnalysis {
    /// Analysed window `[start, end]` in stream seconds.
    pub window: (f64, f64),
    /// Notes in the window.
    pub notes: usize,
    /// Tonic fixed by the user, if any.
    pub tonic_selection: Option<NoteSpelling>,
    /// Overall status.
    pub status: AnalysisStatus,
    /// Ranked candidates (best first).
    pub candidates: Vec<MaqamCandidate>,
    /// The best candidate of the earlier and later halves of the window
    /// differ while both fit well: a modulation may have occurred.
    pub possible_modulation: bool,
}

/// Rolling note history plus scoring.
#[derive(Debug, Clone, Default)]
pub struct MaqamAnalyzer {
    config: AnalyzerConfig,
    history: VecDeque<HistoryNote>,
}

impl MaqamAnalyzer {
    /// Create an analyser.
    pub fn new(config: AnalyzerConfig) -> Self {
        Self {
            config,
            history: VecDeque::new(),
        }
    }

    /// Configuration.
    pub fn config(&self) -> &AnalyzerConfig {
        &self.config
    }

    /// Notes currently in the history.
    pub fn history(&self) -> impl Iterator<Item = &HistoryNote> {
        self.history.iter()
    }

    /// Forget all notes.
    pub fn clear(&mut self) {
        self.history.clear();
    }

    /// Add a completed (or ongoing, with `now_s`) note event.
    pub fn push_event(&mut self, event: &MusicalNoteEvent, now_s: f64) {
        let pitch = match self.config.pitch_space {
            PitchSpace::Player => event.player_pitch.0,
            PitchSpace::Sounding => event.sounding_pitch.0,
        };
        self.push(HistoryNote {
            start_s: event.start_s,
            duration_s: event.duration_s(now_s).max(0.0),
            pitch,
        });
    }

    /// Add a note.
    pub fn push(&mut self, note: HistoryNote) {
        self.history.push_back(note);
    }

    /// Drop notes that ended before `now_s − window_s`.
    pub fn prune(&mut self, now_s: f64) {
        let cutoff = now_s - self.config.window_s;
        while self
            .history
            .front()
            .is_some_and(|n| n.start_s + n.duration_s < cutoff)
        {
            self.history.pop_front();
        }
    }

    /// Score the current window against `dataset`, optionally with a fixed
    /// tonic.
    pub fn analyze(
        &mut self,
        dataset: Option<&MaqamDataset>,
        tonic: Option<NoteSpelling>,
        now_s: f64,
    ) -> MaqamAnalysis {
        self.prune(now_s);
        let notes: Vec<HistoryNote> = self.history.iter().copied().collect();
        let window = (notes.first().map_or(now_s, |n| n.start_s), now_s);
        let mut analysis = MaqamAnalysis {
            window,
            notes: notes.len(),
            tonic_selection: tonic,
            status: AnalysisStatus::InsufficientData,
            candidates: Vec::new(),
            possible_modulation: false,
        };
        let distinct = distinct_classes(&notes);
        if notes.len() < self.config.min_notes || distinct < self.config.min_distinct {
            return analysis;
        }
        let Some(dataset) = dataset.filter(|d| !d.maqamat.is_empty()) else {
            analysis.status = AnalysisStatus::Unknown;
            return analysis;
        };

        let mut candidates = rank(&notes, dataset, tonic, &self.config);
        let best = candidates.first().map_or(0.0, |c| c.score);
        analysis.status = if best < self.config.min_score {
            AnalysisStatus::Unknown
        } else if candidates
            .get(1)
            .is_some_and(|c| best - c.score <= self.config.ambiguity_margin)
        {
            AnalysisStatus::Ambiguous
        } else {
            AnalysisStatus::Likely
        };

        // Modulation check: compare the halves of the window.
        if notes.len() >= 2 * self.config.min_notes {
            let mid = notes.len() / 2;
            let first = rank(&notes[..mid], dataset, tonic, &self.config);
            let second = rank(&notes[mid..], dataset, tonic, &self.config);
            if let (Some(a), Some(b)) = (first.first(), second.first()) {
                analysis.possible_modulation = (a.maqam != b.maqam || a.tonic != b.tonic)
                    && a.score >= self.config.min_score
                    && b.score >= self.config.min_score;
            }
        }

        candidates.truncate(self.config.max_candidates);
        analysis.candidates = candidates;
        analysis
    }
}

/// Number of distinct quarter-tone pitch classes.
fn distinct_classes(notes: &[HistoryNote]) -> usize {
    let mut classes: Vec<i64> = notes
        .iter()
        .map(|n| ((n.pitch * 2.0).round() as i64).rem_euclid(24))
        .collect();
    classes.sort_unstable();
    classes.dedup();
    classes.len()
}

/// Candidate tonic pitch classes (in semitones, quarter-tone resolution):
/// the user's choice, or the most-played pitch classes.
fn candidate_tonics(notes: &[HistoryNote], tonic: Option<NoteSpelling>) -> Vec<NoteSpelling> {
    if let Some(t) = tonic {
        return vec![t];
    }
    let mut weight = [0.0f64; 24];
    for n in notes {
        weight[((n.pitch * 2.0).round() as i64).rem_euclid(24) as usize] += n.duration_s.max(0.05);
    }
    let mut classes: Vec<usize> = (0..24).filter(|&k| weight[k] > 0.0).collect();
    classes.sort_by(|&a, &b| weight[b].total_cmp(&weight[a]));
    classes.truncate(6);
    // Spell each class with its natural/quarter-tone/flat preference from
    // the default table; only the pitch class matters for scoring.
    let table = crate::tuning::SpellingTable::default_24();
    classes.into_iter().map(|k| table.get(k)).collect()
}

fn rank(
    notes: &[HistoryNote],
    dataset: &MaqamDataset,
    tonic: Option<NoteSpelling>,
    config: &AnalyzerConfig,
) -> Vec<MaqamCandidate> {
    let mut out = Vec::new();
    for maqam in &dataset.maqamat {
        let tonics = match tonic {
            Some(t) => vec![t],
            None => candidate_tonics(notes, None),
        };
        for t in tonics {
            out.push(score(notes, dataset, maqam, t, config));
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    // Keep the best tonic per maqam so one maqam does not crowd the list.
    let mut seen = Vec::new();
    out.retain(|c| {
        if seen.contains(&c.maqam) {
            false
        } else {
            seen.push(c.maqam.clone());
            true
        }
    });
    out
}

/// Circular distance in cents within an octave.
fn circ(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(1200.0);
    d.min(1200.0 - d)
}

fn scale_classes(degrees: &[ScaleDegree]) -> Vec<f64> {
    let mut v: Vec<f64> = degrees.iter().map(|d| d.cents.rem_euclid(1200.0)).collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    v
}

fn score(
    notes: &[HistoryNote],
    dataset: &MaqamDataset,
    maqam: &Maqam,
    tonic: NoteSpelling,
    config: &AnalyzerConfig,
) -> MaqamCandidate {
    let w = config.weights;
    let tonic_cents = tonic.quarter_tone_class() as f64 * 50.0;
    let rel = |n: &HistoryNote| (n.pitch * 100.0 - tonic_cents).rem_euclid(1200.0);

    let mut degrees: Vec<ScaleDegree> = maqam.ascending.clone();
    if let Some(desc) = &maqam.descending {
        degrees.extend(desc.iter().copied());
    }
    let classes = scale_classes(&degrees);
    let total_time: f64 = notes.iter().map(|n| n.duration_s.max(0.05)).sum();

    let mut in_time = 0.0;
    let mut foreign_time = 0.0;
    let mut foreign = Vec::new();
    let mut heard = vec![false; classes.len()];
    let mut tonic_time = 0.0;
    for n in notes {
        let r = rel(n);
        let t = n.duration_s.max(0.05);
        let nearest = classes
            .iter()
            .enumerate()
            .map(|(i, &c)| (i, circ(r, c)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match nearest {
            Some((i, d)) if d <= config.tolerance_cents => {
                in_time += t;
                heard[i] = true;
                if circ(r, 0.0) <= config.tolerance_cents {
                    tonic_time += t;
                }
            }
            _ => {
                foreign_time += t;
                let q = (n.pitch * 2.0).round() / 2.0;
                if !foreign.contains(&q) {
                    foreign.push(q);
                }
            }
        }
    }
    let fit = in_time / total_time;
    let foreign_share = foreign_time / total_time;
    let coverage = heard.iter().filter(|&&h| h).count() as f64 / classes.len().max(1) as f64;
    let tonic_share = tonic_time / total_time;

    // The latest note that lasted at least as long as the median duration.
    let mut durations: Vec<f64> = notes.iter().map(|n| n.duration_s).collect();
    durations.sort_by(f64::total_cmp);
    let median = durations[durations.len() / 2];
    let final_rest = notes
        .iter()
        .rev()
        .find(|n| n.duration_s >= median)
        .is_some_and(|n| circ(rel(n), 0.0) <= config.tolerance_cents);

    let resting: Vec<f64> = maqam
        .resting_steps
        .iter()
        .filter_map(|s| maqam.ascending.iter().find(|d| d.step == *s))
        .map(|d| d.cents.rem_euclid(1200.0))
        .collect();
    let resting_time: f64 = notes
        .iter()
        .filter(|n| {
            resting
                .iter()
                .any(|&c| circ(rel(n), c) <= config.tolerance_cents)
        })
        .map(|n| n.duration_s.max(0.05))
        .sum();
    let resting_share = if resting.is_empty() {
        0.0
    } else {
        resting_time / total_time
    };

    let mut supporting = vec![
        Evidence::ScaleFit { share: fit },
        Evidence::Coverage {
            heard: heard.iter().filter(|&&h| h).count(),
            total: classes.len(),
        },
        Evidence::Tonic {
            share: tonic_share,
            final_rest,
        },
    ];
    if !resting.is_empty() {
        supporting.push(Evidence::RestingTones {
            share: resting_share,
        });
    }

    // Ajnas completeness.
    let mut ajnas_score = 0.0;
    let mut ajnas_count = 0;
    for p in &maqam.ajnas {
        let Some(jins) = dataset.jins(&p.jins) else {
            continue;
        };
        let Some(base) = maqam.ascending.iter().find(|d| d.step == p.on_step) else {
            continue;
        };
        let jins_classes: Vec<f64> = jins
            .degrees
            .iter()
            .map(|d| (base.cents + d.cents).rem_euclid(1200.0))
            .collect();
        let got = jins_classes
            .iter()
            .filter(|&&c| {
                notes
                    .iter()
                    .any(|n| circ(rel(n), c) <= config.tolerance_cents)
            })
            .count();
        let completeness = got as f64 / jins_classes.len().max(1) as f64;
        ajnas_score += completeness;
        ajnas_count += 1;
        supporting.push(Evidence::Jins {
            jins: jins.id.clone(),
            completeness,
        });
    }
    let ajnas_avg = if ajnas_count > 0 {
        ajnas_score / ajnas_count as f64
    } else {
        0.0
    };

    // Phrases: subsequence of scale steps (tonic-relative) in the melody.
    let step_of = |n: &HistoryNote| -> Option<i32> {
        let r = n.pitch * 100.0 - tonic_cents;
        let octave = (r / 1200.0).floor() as i32;
        let within = r.rem_euclid(1200.0);
        maqam
            .ascending
            .iter()
            .filter(|d| d.step < 7)
            .find(|d| circ(within, d.cents) <= config.tolerance_cents)
            .map(|d| d.step as i32 + 7 * octave)
    };
    let steps: Vec<Option<i32>> = notes.iter().map(step_of).collect();
    let mut phrases_found = 0;
    for (i, ph) in maqam.phrases.iter().enumerate() {
        if ph.steps.len() < 2 {
            continue;
        }
        let pattern: Vec<i32> = ph.steps.iter().map(|&s| s as i32).collect();
        let found = steps.windows(pattern.len()).any(|win| {
            let base = match win[0] {
                Some(b) => b - pattern[0],
                None => return false,
            };
            win.iter()
                .zip(&pattern)
                .all(|(s, p)| *s == Some(p + base) && base % 7 == 0)
        });
        if found {
            phrases_found += 1;
            supporting.push(Evidence::Phrase { index: i });
        }
    }

    let mut contradicting = Vec::new();
    if foreign_share > 0.0 {
        contradicting.push(Evidence::ForeignNotes {
            share: foreign_share,
            pitches: foreign,
        });
    }

    let score = w.fit * fit - w.foreign * foreign_share
        + w.coverage * coverage
        // Tonic term in [0, 1]: half from time on the tonic (saturating at a
        // 50 % share), half from ending on it.
        + w.tonic * (0.5 * (tonic_share * 2.0).min(1.0) + if final_rest { 0.5 } else { 0.0 })
        + w.resting * resting_share
        + w.ajnas * ajnas_avg
        + w.phrase * phrases_found.min(3) as f64;

    MaqamCandidate {
        maqam: maqam.id.clone(),
        tonic,
        score,
        supporting,
        contradicting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::{Accidental, Degree};

    const FIXTURE: &str = include_str!("../../tests/fixtures/synthetic-maqam-dataset.json");

    fn dataset() -> MaqamDataset {
        MaqamDataset::from_json(FIXTURE).unwrap()
    }

    fn analyzer() -> MaqamAnalyzer {
        MaqamAnalyzer::new(AnalyzerConfig::default())
    }

    /// Push a melody of (pitch, seconds), back to back, starting at t0.
    fn play(a: &mut MaqamAnalyzer, t0: f64, melody: &[(f64, f64)]) -> f64 {
        let mut t = t0;
        for &(pitch, dur) in melody {
            a.push(HistoryNote {
                start_s: t,
                duration_s: dur,
                pitch,
            });
            t += dur;
        }
        t
    }

    fn sp(d: Degree, q: i8) -> NoteSpelling {
        NoteSpelling::new(d, Accidental::new(q).unwrap())
    }

    // SYNTHETIC "TEST maqam A" on Do: 0 200 350 500 700 900 1050 1200 cents
    // → pitches (from Do4 = 60): 60 62 63.5 65 67 69 70.5 72.
    const A_ON_DO: [f64; 8] = [60.0, 62.0, 63.5, 65.0, 67.0, 69.0, 70.5, 72.0];

    #[test]
    fn insufficient_data_is_reported() {
        let mut a = analyzer();
        let end = play(&mut a, 0.0, &[(60.0, 0.5), (62.0, 0.5), (63.5, 0.5)]);
        let r = a.analyze(Some(&dataset()), None, end);
        assert_eq!(r.status, AnalysisStatus::InsufficientData);
        assert!(r.candidates.is_empty());
    }

    #[test]
    fn no_dataset_gives_unknown_not_a_guess() {
        let mut a = analyzer();
        let melody: Vec<(f64, f64)> = A_ON_DO.iter().map(|&p| (p, 0.4)).collect();
        let end = play(&mut a, 0.0, &melody);
        let r = a.analyze(None, None, end);
        assert_eq!(r.status, AnalysisStatus::Unknown);
        assert!(r.candidates.is_empty());
    }

    #[test]
    fn scale_material_ranks_its_maqam_first() {
        let mut a = analyzer();
        let mut melody: Vec<(f64, f64)> = A_ON_DO.iter().map(|&p| (p, 0.4)).collect();
        melody.extend([
            (67.0, 0.6),
            (65.0, 0.4),
            (63.5, 0.4),
            (62.0, 0.4),
            (60.0, 1.2),
        ]);
        let end = play(&mut a, 0.0, &melody);
        let r = a.analyze(Some(&dataset()), None, end);
        assert_eq!(r.status, AnalysisStatus::Likely, "{r:#?}");
        let best = &r.candidates[0];
        assert_eq!(best.maqam, "test-maqam-a");
        assert_eq!(best.tonic, sp(Degree::Do, 0));
        assert!(best.contradicting.is_empty());
    }

    #[test]
    fn foreign_notes_are_contradicting_evidence() {
        let mut a = analyzer();
        // Maqam A on Do but with Mi natural (64) instead of Mi half-flat.
        let melody: Vec<(f64, f64)> = [60.0, 62.0, 64.0, 65.0, 67.0, 64.0, 62.0, 60.0]
            .iter()
            .map(|&p| (p, 0.5))
            .collect();
        let end = play(&mut a, 0.0, &melody);
        let r = a.analyze(Some(&dataset()), Some(sp(Degree::Do, 0)), end);
        let c = r
            .candidates
            .iter()
            .find(|c| c.maqam == "test-maqam-a")
            .unwrap();
        assert!(c.contradicting.iter().any(
            |e| matches!(e, Evidence::ForeignNotes { pitches, .. } if pitches.contains(&64.0))
        ));
    }

    #[test]
    fn shared_notes_without_distinguishing_degrees_are_ambiguous() {
        // A dataset with two maqamat that differ only in a degree that is
        // never played: the result must be ambiguous, not a forced pick.
        let mut ds = dataset();
        let mut twin = ds.maqamat[0].clone();
        twin.id = "test-maqam-a-twin".into();
        twin.related.clear();
        twin.modulations.clear();
        twin.ascending[6].cents = 1000.0; // differs only at step 6
        ds.maqamat.push(twin);
        ds.validate().unwrap();

        let mut a = analyzer();
        let melody: Vec<(f64, f64)> = [60.0, 62.0, 63.5, 65.0, 67.0, 65.0, 63.5, 62.0, 60.0]
            .iter()
            .map(|&p| (p, 0.5))
            .collect();
        let end = play(&mut a, 0.0, &melody);
        let r = a.analyze(Some(&ds), Some(sp(Degree::Do, 0)), end);
        assert_eq!(r.status, AnalysisStatus::Ambiguous, "{r:#?}");
    }

    #[test]
    fn fixed_tonic_changes_interpretation() {
        let mut a = analyzer();
        let melody: Vec<(f64, f64)> = A_ON_DO.iter().map(|&p| (p, 0.4)).collect();
        let end = play(&mut a, 0.0, &melody);
        let on_do = a.analyze(Some(&dataset()), Some(sp(Degree::Do, 0)), end);
        let on_re = a.analyze(Some(&dataset()), Some(sp(Degree::Re, 0)), end);
        let s = |r: &MaqamAnalysis| {
            r.candidates
                .iter()
                .find(|c| c.maqam == "test-maqam-a")
                .unwrap()
                .score
        };
        assert!(s(&on_do) > s(&on_re) + 0.3);
        assert_eq!(on_re.tonic_selection, Some(sp(Degree::Re, 0)));
    }

    #[test]
    fn rolling_window_forgets_old_notes() {
        let mut a = MaqamAnalyzer::new(AnalyzerConfig {
            window_s: 5.0,
            ..AnalyzerConfig::default()
        });
        let melody: Vec<(f64, f64)> = A_ON_DO.iter().map(|&p| (p, 0.5)).collect();
        let end = play(&mut a, 0.0, &melody);
        assert_eq!(a.analyze(Some(&dataset()), None, end).notes, 8);
        let r = a.analyze(Some(&dataset()), None, end + 10.0);
        assert_eq!(r.notes, 0);
        assert_eq!(r.status, AnalysisStatus::InsufficientData);
    }

    #[test]
    fn transposition_uses_player_pitch_by_default() {
        use crate::interpret::PitchContext;
        use crate::pitch::{IntervalUnit, Pitch, Transposition};
        use crate::tracking::{NoteTracker, TrackerConfig};
        // Player plays maqam A on Do on an instrument tuned −2: the sounding
        // pitches are on Si♭, but the analysis (player space) finds Do.
        let ctx = PitchContext {
            transposition: Transposition::new(-2.0, IntervalUnit::Semitone),
            ..PitchContext::default()
        };
        let mut tracker = NoteTracker::new(TrackerConfig::default(), ctx.clone());
        let mut a = analyzer();
        let mut t = 0.0;
        let hop = 0.01;
        let mut melody = A_ON_DO.to_vec();
        melody.extend([67.0, 65.0, 63.5, 62.0, 60.0]);
        for (k, &player) in melody.iter().enumerate() {
            let sounding = ctx.transposition.player_to_sounding(Pitch(player));
            for i in 0..40 {
                let obs = crate::events::PitchObservation {
                    time_s: t,
                    window_s: 0.085,
                    status: crate::events::ObservationStatus::Voiced,
                    frequency_hz: Some(sounding.to_frequency(ctx.reference)),
                    confidence: 0.95,
                    level_db: -20.0,
                };
                let onset = (i == 0 && k > 0).then_some(t - 0.1);
                let u = tracker.update(&obs, onset);
                if let Some(e) = &u.ended {
                    a.push_event(e, t);
                }
                t += hop;
            }
        }
        if let Some(e) = tracker.flush(t) {
            a.push_event(&e, t);
        }
        let r = a.analyze(Some(&dataset()), None, t);
        assert_eq!(r.candidates[0].maqam, "test-maqam-a");
        assert_eq!(r.candidates[0].tonic, sp(Degree::Do, 0), "{r:#?}");
    }

    #[test]
    fn modulation_is_flagged_when_halves_disagree() {
        let mut a = MaqamAnalyzer::new(AnalyzerConfig {
            min_notes: 5,
            ..AnalyzerConfig::default()
        });
        // First half: maqam A on Do. Second half: maqam B on Re
        // (0 100 400 500 700 → 62 63 66 67 69).
        let mut melody: Vec<(f64, f64)> = [60.0, 62.0, 63.5, 65.0, 67.0, 63.5, 60.0]
            .iter()
            .map(|&p| (p, 0.5))
            .collect();
        melody.extend(
            [62.0, 63.0, 66.0, 67.0, 69.0, 66.0, 62.0]
                .iter()
                .map(|&p| (p, 0.5)),
        );
        let end = play(&mut a, 0.0, &melody);
        let r = a.analyze(Some(&dataset()), None, end);
        assert!(r.possible_modulation, "{r:#?}");
    }
}
