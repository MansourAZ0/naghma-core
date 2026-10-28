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
    /// Weight of time spent on the tonic (saturating at a 50 % share).
    pub tonic: f64,
    /// Weight of the latest long note resting on the tonic.
    pub final_rest: f64,
    /// Weight of resting-tone emphasis.
    pub resting: f64,
    /// Weight of jins completeness.
    pub ajnas: f64,
    /// Bonus per matched characteristic phrase (capped at 3).
    pub phrase: f64,
    /// Bonus for the maqam and tonic that led the previous analysis. A
    /// maqam lasts longer than one analysis step, so a near-tie keeps the
    /// current reading instead of flickering; clear evidence still wins.
    #[serde(default)]
    pub continuity: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            fit: 1.0,
            foreign: 1.5,
            coverage: 0.5,
            tonic: 0.4,
            final_rest: 0.0,
            resting: 0.2,
            ajnas: 0.3,
            phrase: 0.15,
            continuity: 0.1,
        }
    }
}

/// Parameters of [`MaqamAnalyzer`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyzerConfig {
    /// Length of the rolling window scored for the maqam, in seconds.
    ///
    /// Short enough that a taqsim's sections are not mixed together: notes
    /// from a passage in another maqam leave the window quickly.
    pub window_s: f64,
    /// Seconds of playing used to estimate the instrument's tuning offset.
    /// Tuning belongs to the instrument, not the maqam, so it is estimated
    /// over a longer span than [`Self::window_s`] (never shorter).
    #[serde(default = "default_tuning_window")]
    pub tuning_window_s: f64,
    /// Minimum notes in the window before any ranking is reported.
    pub min_notes: usize,
    /// Minimum distinct pitch classes before any ranking is reported.
    pub min_distinct: usize,
    /// Accepted intonation error, in cents (1 cent = 1 % of a semitone, so
    /// 30 cents = 30 % of a semitone).
    ///
    /// Membership is soft: a played pitch within half of this of a scale
    /// degree counts fully as that degree, one exactly this far counts
    /// half, and credit fades smoothly to nothing at 1.5 × this distance.
    /// Real fingers and strings are never exact; the fade avoids an
    /// all-or-nothing cliff at the boundary.
    pub tolerance_cents: f64,
    /// Notes shorter than this (seconds) are ignored as transients.
    pub min_note_s: f64,
    /// Global tuning offset of the instrument in cents, subtracted from
    /// every note before scoring. `None` estimates it from the window
    /// (see [`estimate_tuning_offset`]); `Some(0.0)` disables compensation.
    pub tuning_offset_cents: Option<f64>,
    /// Largest offset the estimator may return, cents. Keep below 25: a
    /// larger shift could reinterpret naturals as quarter-tones.
    pub max_tuning_offset_cents: f64,
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
            window_s: 10.0,
            tuning_window_s: default_tuning_window(),
            min_notes: 6,
            min_distinct: 4,
            tolerance_cents: 30.0,
            min_note_s: 0.15,
            tuning_offset_cents: None,
            max_tuning_offset_cents: 20.0,
            ambiguity_margin: 0.08,
            min_score: 0.6,
            max_candidates: 5,
            pitch_space: PitchSpace::Player,
            weights: ScoreWeights::default(),
        }
    }
}

fn default_tuning_window() -> f64 {
    60.0
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
    /// Selected alternative illustration; `None` means the primary formula.
    pub variant: Option<String>,
    /// Explicit anchor register for nonperiodic illustrations.
    pub tonic_octave: Option<i8>,
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
    /// Notes in the window (after ignoring transients).
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
    /// Tuning offset (cents) subtracted from the notes before scoring:
    /// estimated, or the configured fixed value.
    #[serde(default)]
    pub tuning_offset_cents: f64,
}

/// Rolling note history plus scoring.
#[derive(Debug, Clone, Default)]
pub struct MaqamAnalyzer {
    config: AnalyzerConfig,
    history: VecDeque<HistoryNote>,
    /// Maqam id and tonic pitch class that led the previous analysis.
    leader: Option<(String, i64)>,
}

impl MaqamAnalyzer {
    /// Create an analyser.
    pub fn new(config: AnalyzerConfig) -> Self {
        Self {
            config,
            history: VecDeque::new(),
            leader: None,
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
        self.leader = None;
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

    /// Drop notes that ended before both the maqam window and the tuning
    /// window.
    pub fn prune(&mut self, now_s: f64) {
        let cutoff = now_s - self.config.window_s.max(self.config.tuning_window_s);
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
        let config = &self.config;
        let context: Vec<HistoryNote> = self
            .history
            .iter()
            .filter(|n| n.duration_s >= config.min_note_s)
            .copied()
            .collect();
        let cutoff = now_s - config.window_s;
        let raw: Vec<HistoryNote> = context
            .iter()
            .filter(|n| n.start_s + n.duration_s >= cutoff)
            .copied()
            .collect();
        let window = (raw.first().map_or(now_s, |n| n.start_s), now_s);
        let offset = config.tuning_offset_cents.unwrap_or_else(|| {
            estimate_tuning_offset(&context, config.max_tuning_offset_cents).unwrap_or(0.0)
        });
        let notes = prepare(&raw, offset);
        let mut analysis = MaqamAnalysis {
            window,
            notes: notes.len(),
            tonic_selection: tonic,
            status: AnalysisStatus::InsufficientData,
            candidates: Vec::new(),
            possible_modulation: false,
            tuning_offset_cents: offset,
        };
        let distinct = distinct_classes(&notes);
        if notes.len() < config.min_notes || distinct < config.min_distinct {
            self.leader = None;
            return analysis;
        }
        let Some(dataset) = dataset.filter(|d| !d.maqamat.is_empty()) else {
            self.leader = None;
            analysis.status = AnalysisStatus::Unknown;
            return analysis;
        };

        let mut candidates = rank(&notes, dataset, tonic, config);
        if let Some((maqam, class)) = &self.leader {
            if let Some(c) = candidates
                .iter_mut()
                .find(|c| &c.maqam == maqam && c.tonic.quarter_tone_class() == *class)
            {
                c.score += config.weights.continuity;
                candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
            }
        }
        self.leader = candidates
            .first()
            .map(|c| (c.maqam.clone(), c.tonic.quarter_tone_class()));
        let best = candidates.first().map_or(0.0, |c| c.score);
        analysis.status = if best < config.min_score {
            AnalysisStatus::Unknown
        } else if candidates
            .get(1)
            .is_some_and(|c| best - c.score <= config.ambiguity_margin)
        {
            AnalysisStatus::Ambiguous
        } else {
            AnalysisStatus::Likely
        };

        // Modulation check: compare the halves of the window.
        if notes.len() >= 2 * config.min_notes {
            let mid = notes.len() / 2;
            let first = rank(&notes[..mid], dataset, tonic, config);
            let second = rank(&notes[mid..], dataset, tonic, config);
            if let (Some(a), Some(b)) = (first.first(), second.first()) {
                analysis.possible_modulation = (a.maqam != b.maqam || a.tonic != b.tonic)
                    && a.score >= config.min_score
                    && b.score >= config.min_score;
            }
        }

        candidates.truncate(config.max_candidates);
        analysis.candidates = candidates;
        analysis
    }
}

/// Estimate an instrument's global tuning offset, in cents, from played
/// notes.
///
/// Arabic music uses quarter-tones, but most played time is on
/// 12-EDO-natural degrees, so the offset is the shift of the semitone grid
/// that best fits the notes: the maximum of a duration-weighted kernel
/// density (σ = 10 cents) of note positions relative to the nearest
/// semitone, searched within `±max_cents` and refined by a local weighted
/// mean. Notes far from any semitone (such as half-flats) barely contribute.
///
/// Returns `None` with less than one second of material. `max_cents` should
/// stay below 25: a 50-cent shift would turn every natural into a
/// quarter-tone.
pub fn estimate_tuning_offset(notes: &[HistoryNote], max_cents: f64) -> Option<f64> {
    const SIGMA: f64 = 10.0;
    let max_cents = max_cents.clamp(0.0, 49.0);
    let points: Vec<(f64, f64)> = notes
        .iter()
        .filter(|n| n.duration_s > 0.0 && n.pitch.is_finite())
        .map(|n| (n.pitch * 100.0, n.duration_s.min(2.0)))
        .collect();
    let total: f64 = points.iter().map(|p| p.1).sum();
    if total < 1.0 {
        return None;
    }
    let wrap = |x: f64| (x + 50.0).rem_euclid(100.0) - 50.0;
    let density = |d: f64| -> f64 {
        points
            .iter()
            .map(|&(c, w)| w * (-0.5 * (wrap(c - d) / SIGMA).powi(2)).exp())
            .sum()
    };
    let steps = (max_cents * 2.0).round() as i64;
    let mut best: (f64, f64) = (0.0, f64::NEG_INFINITY);
    for k in -steps..=steps {
        let d = k as f64 * 0.5;
        let s = density(d);
        // Prefer the smaller offset on (near-)ties.
        if s > best.1 + 1e-9 || ((s - best.1).abs() <= 1e-9 && d.abs() < best.0.abs()) {
            best = (d, s);
        }
    }
    let (mut num, mut den) = (0.0, 0.0);
    for &(c, w) in &points {
        let r = wrap(c - best.0);
        if r.abs() < 2.0 * SIGMA {
            num += w * r;
            den += w;
        }
    }
    let refined = if den > 0.0 {
        best.0 + num / den
    } else {
        best.0
    };
    Some(refined.clamp(-max_cents, max_cents))
}

/// A note prepared for scoring.
#[derive(Debug, Clone, Copy)]
struct Prepared {
    /// Absolute pitch in cents after tuning compensation.
    cents: f64,
    /// Evidence weight (duration, at least 50 ms).
    weight: f64,
    /// Duration, seconds.
    duration: f64,
}

fn prepare(notes: &[HistoryNote], offset: f64) -> Vec<Prepared> {
    let mut prepared: Vec<(f64, Prepared)> = notes
        .iter()
        .map(|n| {
            (
                n.start_s,
                Prepared {
                    cents: n.pitch * 100.0 - offset,
                    weight: n.duration_s.max(0.05),
                    duration: n.duration_s,
                },
            )
        })
        .collect();
    prepared.sort_by(|a, b| a.0.total_cmp(&b.0));
    prepared.into_iter().map(|(_, p)| p).collect()
}

/// Number of distinct quarter-tone pitch classes.
fn distinct_classes(notes: &[Prepared]) -> usize {
    let mut classes: Vec<i64> = notes
        .iter()
        .map(|n| ((n.cents / 50.0).round() as i64).rem_euclid(24))
        .collect();
    classes.sort_unstable();
    classes.dedup();
    classes.len()
}

/// Candidate tonic pitch classes (quarter-tone resolution): the
/// most-played classes.
fn candidate_tonics(notes: &[Prepared]) -> Vec<NoteSpelling> {
    let mut weight = [0.0f64; 24];
    for n in notes {
        weight[((n.cents / 50.0).round() as i64).rem_euclid(24) as usize] += n.weight;
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
    notes: &[Prepared],
    dataset: &MaqamDataset,
    tonic: Option<NoteSpelling>,
    config: &AnalyzerConfig,
) -> Vec<MaqamCandidate> {
    let tonics = match tonic {
        Some(t) => vec![t],
        None => candidate_tonics(notes),
    };
    let mut out = Vec::new();
    for maqam in &dataset.maqamat {
        for &t in &tonics {
            for variant in
                std::iter::once(None).chain(maqam.variants.iter().map(|v| Some(v.id.as_str())))
            {
                if maqam.octave_repeating {
                    out.extend(score(notes, dataset, maqam, t, variant, None, config));
                } else {
                    let low = notes.iter().map(|n| n.cents).fold(f64::INFINITY, f64::min);
                    let high = notes
                        .iter()
                        .map(|n| n.cents)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let first = ((low / 1200.0).floor() as i32 - 3).max(-128);
                    let last = ((high / 1200.0).floor() as i32).min(125);
                    for octave in first..=last {
                        out.extend(score(
                            notes,
                            dataset,
                            maqam,
                            t,
                            variant,
                            Some(octave as i8),
                            config,
                        ));
                    }
                }
            }
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

/// Soft membership of a pitch `distance` cents from a target accepted with
/// `tolerance`: 1 up to half the tolerance, ½ at the tolerance, raised-cosine
/// fade to 0 at 1.5 × the tolerance.
pub fn membership(distance: f64, tolerance: f64) -> f64 {
    let d = distance.abs();
    let lo = 0.5 * tolerance;
    let hi = 1.5 * tolerance;
    if d <= lo {
        1.0
    } else if d >= hi {
        0.0
    } else {
        0.5 * (1.0 + (std::f64::consts::PI * (d - lo) / (hi - lo)).cos())
    }
}

/// A scale degree as a scoring target.
#[derive(Debug, Clone, Copy)]
struct Target {
    /// Cents above the tonic (pitch class for periodic maqamat).
    cents: f64,
    /// Accepted error for this degree.
    tolerance: f64,
}

fn targets(degrees: &[ScaleDegree], repeating: bool, tolerance: f64) -> Vec<Target> {
    let mut v: Vec<f64> = degrees
        .iter()
        .map(|d| {
            if repeating {
                d.cents.rem_euclid(1200.0)
            } else {
                d.cents
            }
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    v.into_iter()
        .map(|cents| Target { cents, tolerance })
        .collect()
}

fn score(
    notes: &[Prepared],
    dataset: &MaqamDataset,
    maqam: &Maqam,
    tonic: NoteSpelling,
    variant: Option<&str>,
    tonic_octave: Option<i8>,
    config: &AnalyzerConfig,
) -> Option<MaqamCandidate> {
    let w = config.weights;
    let repeating = maqam.octave_repeating;
    let tonic_cents = tonic_octave.map_or_else(
        || tonic.quarter_tone_class() as f64 * 50.0,
        |octave| {
            crate::notation::SpelledNote::new(tonic.degree, tonic.accidental, octave)
                .nominal_pitch()
                .0
                * 100.0
        },
    );
    let scale = maqam.scale(variant).expect("variant comes from this maqam");
    let placements = maqam
        .placements(variant)
        .expect("variant comes from this maqam");
    let mut degrees = scale.to_vec();
    if variant.is_none() {
        if let Some(desc) = &maqam.descending {
            degrees.extend(desc.iter().copied());
        }
    }
    let tol = config.tolerance_cents;
    let classes = targets(&degrees, repeating, tol);
    // Nonperiodic illustrations document a register. Notes inside it are
    // compared exactly, so register-specific degrees stay distinct. Notes
    // outside it (for example below the tonic) are compared, an octave at a
    // time, with the nearest documented register: the data says nothing
    // more specific about them. An anchor is only considered when most of
    // the played time lies inside its documented register, so folding can
    // never move a whole melody into a different register.
    let span = (
        classes.first().map_or(0.0, |c| c.cents),
        classes.last().map_or(0.0, |c| c.cents),
    );
    let margin = 1.5 * config.tolerance_cents;
    if !repeating {
        let total: f64 = notes.iter().map(|n| n.weight).sum();
        let inside: f64 = notes
            .iter()
            .filter(|n| {
                let r = n.cents - tonic_cents;
                r >= span.0 - margin && r <= span.1 + margin
            })
            .map(|n| n.weight)
            .sum();
        if inside < 0.5 * total {
            return None;
        }
    }
    let rel = |n: &Prepared| -> f64 {
        let r = n.cents - tonic_cents;
        if repeating {
            r.rem_euclid(1200.0)
        } else if r < span.0 - margin {
            r + 1200.0 * ((span.0 - margin - r) / 1200.0).ceil()
        } else if r > span.1 + margin {
            r - 1200.0 * ((r - span.1 - margin) / 1200.0).ceil()
        } else {
            r
        }
    };
    let distance = |a: f64, b: f64| if repeating { circ(a, b) } else { (a - b).abs() };
    // Best matching target and its membership.
    let best_match = |r: f64, targets: &[Target]| -> Option<(usize, f64)> {
        targets
            .iter()
            .enumerate()
            .map(|(i, t)| (i, membership(distance(r, t.cents), t.tolerance)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    };
    let on = |r: f64, c: f64| membership(distance(r, c), tol);
    let total_time: f64 = notes.iter().map(|n| n.weight).sum();
    // A degree counts as heard after this much (membership-weighted) time,
    // so a single transient does not complete a jins.
    let heard_s = 0.12;

    let mut in_time = 0.0;
    let mut foreign_time = 0.0;
    let mut foreign = Vec::new();
    let mut degree_time = vec![0.0; classes.len()];
    let mut tonic_time = 0.0;
    for n in notes {
        let r = rel(n);
        let m = best_match(r, &classes).map_or(0.0, |(i, m)| {
            degree_time[i] += n.weight * m;
            m
        });
        in_time += n.weight * m;
        foreign_time += n.weight * (1.0 - m);
        if m < 0.5 {
            let q = (n.cents / 50.0).round() / 2.0;
            if !foreign.contains(&q) {
                foreign.push(q);
            }
        }
        tonic_time += n.weight * on(r, 0.0);
    }
    let heard = degree_time.iter().filter(|&&t| t >= heard_s).count();
    let fit = in_time / total_time;
    let foreign_share = foreign_time / total_time;
    let coverage = heard as f64 / classes.len().max(1) as f64;
    let tonic_share = tonic_time / total_time;

    // The latest note that lasted at least as long as the median duration.
    let mut durations: Vec<f64> = notes.iter().map(|n| n.duration).collect();
    durations.sort_by(f64::total_cmp);
    let median = durations[durations.len() / 2];
    let final_rest = notes
        .iter()
        .rev()
        .find(|n| n.duration >= median)
        .is_some_and(|n| on(rel(n), 0.0) >= 0.5);

    let resting: Vec<f64> = maqam
        .resting_steps
        .iter()
        .filter_map(|s| scale.iter().find(|d| d.step == *s))
        .map(|d| {
            if repeating {
                d.cents.rem_euclid(1200.0)
            } else {
                d.cents
            }
        })
        .collect();
    let resting_time: f64 = notes
        .iter()
        .map(|n| {
            let r = rel(n);
            n.weight * resting.iter().map(|&c| on(r, c)).fold(0.0, f64::max)
        })
        .sum();
    let resting_share = if resting.is_empty() {
        0.0
    } else {
        resting_time / total_time
    };

    let mut supporting = vec![
        Evidence::ScaleFit { share: fit },
        Evidence::Coverage {
            heard,
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
    for p in placements {
        let Some(jins) = dataset.jins(&p.jins) else {
            continue;
        };
        let Some(base) = scale.iter().find(|d| d.step == p.on_step) else {
            continue;
        };
        let got = jins
            .degrees
            .iter()
            .map(|d| {
                let c = base.cents + d.cents;
                if repeating { c.rem_euclid(1200.0) } else { c }
            })
            .filter(|&c| notes.iter().map(|n| n.weight * on(rel(n), c)).sum::<f64>() >= heard_s)
            .count();
        let completeness = got as f64 / jins.degrees.len().max(1) as f64;
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
    let step_of = |n: &Prepared| -> Option<i32> {
        let r = n.cents - tonic_cents;
        if !repeating {
            return scale
                .iter()
                .find(|d| on(r, d.cents) >= 0.5)
                .map(|d| i32::from(d.step));
        }
        let octave = (r / 1200.0).floor() as i32;
        let within = r.rem_euclid(1200.0);
        scale
            .iter()
            .filter(|d| d.step < 7)
            .find(|d| on(within, d.cents) >= 0.5)
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
            win.iter().zip(&pattern).all(|(s, p)| {
                *s == Some(p + base) && (if repeating { base % 7 == 0 } else { base == 0 })
            })
        });
        if found {
            phrases_found += 1;
            supporting.push(Evidence::Phrase { index: i });
        }
    }

    let mut contradicting = Vec::new();
    if foreign_share > 1e-9 {
        contradicting.push(Evidence::ForeignNotes {
            share: foreign_share,
            pitches: foreign,
        });
    }

    let score = w.fit * fit - w.foreign * foreign_share
        + w.coverage * coverage
        + w.tonic * (tonic_share * 2.0).min(1.0)
        + if final_rest { w.final_rest } else { 0.0 }
        + w.resting * resting_share
        + w.ajnas * ajnas_avg
        + w.phrase * phrases_found.min(3) as f64;

    Some(MaqamCandidate {
        maqam: maqam.id.clone(),
        tonic,
        variant: variant.map(str::to_owned),
        tonic_octave,
        score,
        supporting,
        contradicting,
    })
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

    /// Maqam A on Do, ending on the tonic, every pitch shifted by `cents`.
    fn detuned_melody(cents: f64) -> Vec<(f64, f64)> {
        let mut melody: Vec<(f64, f64)> = A_ON_DO.iter().map(|&p| (p, 0.4)).collect();
        melody.extend([
            (67.0, 0.6),
            (65.0, 0.4),
            (63.5, 0.4),
            (62.0, 0.4),
            (60.0, 1.2),
        ]);
        melody
            .into_iter()
            .map(|(p, d)| (p + cents / 100.0, d))
            .collect()
    }

    #[test]
    fn membership_is_soft_with_half_credit_at_the_tolerance() {
        assert_eq!(membership(0.0, 30.0), 1.0);
        assert_eq!(membership(15.0, 30.0), 1.0);
        assert!((membership(30.0, 30.0) - 0.5).abs() < 1e-12);
        assert!((membership(-30.0, 30.0) - 0.5).abs() < 1e-12);
        assert_eq!(membership(45.0, 30.0), 0.0);
        assert!(membership(25.0, 30.0) > membership(35.0, 30.0));
    }

    #[test]
    fn no_cliff_at_the_tolerance_boundary() {
        // One note 29 vs 31 cents off: the hard 30-cent cutoff used to flip
        // it from "in scale" to "foreign"; now the fit barely changes.
        let fit = |off: f64| {
            let mut a = MaqamAnalyzer::new(AnalyzerConfig {
                tuning_offset_cents: Some(0.0),
                ..AnalyzerConfig::default()
            });
            let mut melody = detuned_melody(0.0);
            melody[2].0 += off / 100.0;
            let end = play(&mut a, 0.0, &melody);
            let r = a.analyze(Some(&dataset()), Some(sp(Degree::Do, 0)), end);
            let c = r
                .candidates
                .iter()
                .find(|c| c.maqam == "test-maqam-a")
                .unwrap()
                .clone();
            c.supporting
                .iter()
                .find_map(|e| match e {
                    Evidence::ScaleFit { share } => Some(*share),
                    _ => None,
                })
                .unwrap()
        };
        assert!((fit(29.0) - fit(31.0)).abs() < 0.01);
    }

    #[test]
    fn tuning_offset_is_found_and_quarter_tones_do_not_pull_it() {
        // Maqam A contains two half-flats; the instrument is 12 cents sharp.
        let notes: Vec<HistoryNote> = detuned_melody(12.0)
            .into_iter()
            .enumerate()
            .map(|(i, (pitch, d))| HistoryNote {
                start_s: i as f64,
                duration_s: d,
                pitch,
            })
            .collect();
        let est = estimate_tuning_offset(&notes, 20.0).unwrap();
        assert!((est - 12.0).abs() < 1.0, "{est}");
        let flat: Vec<HistoryNote> = notes
            .iter()
            .map(|n| HistoryNote {
                pitch: n.pitch - 0.2,
                ..*n
            })
            .collect();
        let est = estimate_tuning_offset(&flat, 20.0).unwrap();
        assert!((est + 8.0).abs() < 1.0, "{est}");
    }

    #[test]
    fn tuning_offset_is_bounded_and_needs_material() {
        let notes: Vec<HistoryNote> = (0..8)
            .map(|i| HistoryNote {
                start_s: i as f64,
                duration_s: 0.5,
                pitch: 60.0 + i as f64 + 0.4,
            })
            .collect();
        let est = estimate_tuning_offset(&notes, 20.0).unwrap();
        assert!(
            est.abs() <= 20.0,
            "a 40-cent shift must not be followed: {est}"
        );
        assert_eq!(estimate_tuning_offset(&notes[..1], 20.0), None);
    }

    #[test]
    fn a_detuned_instrument_still_names_the_right_tonic() {
        // 27 cents sharp: without compensation every note rounds up to the
        // next quarter-tone and the tonic is misnamed as Do half-sharp.
        let run = |offset: Option<f64>| {
            let mut a = MaqamAnalyzer::new(AnalyzerConfig {
                tuning_offset_cents: offset,
                ..AnalyzerConfig::default()
            });
            let end = play(&mut a, 0.0, &detuned_melody(27.0));
            a.analyze(Some(&dataset()), None, end)
        };
        let fixed = run(Some(0.0));
        assert_eq!(fixed.candidates[0].maqam, "test-maqam-a");
        assert_ne!(fixed.candidates[0].tonic, sp(Degree::Do, 0));
        let auto = run(None);
        assert_eq!(auto.candidates[0].maqam, "test-maqam-a", "{auto:#?}");
        assert_eq!(auto.candidates[0].tonic, sp(Degree::Do, 0));
        assert!((auto.tuning_offset_cents - 20.0).abs() < 1e-9);
        assert!(auto.candidates[0].contradicting.is_empty());
    }

    #[test]
    fn short_transients_are_ignored() {
        let mut melody = detuned_melody(0.0);
        // Pluck transients a quarter-tone off, 50 ms each.
        for k in [3, 6, 9] {
            melody.insert(k, (64.5, 0.05));
        }
        let run = |min_note_s: f64| {
            let mut a = MaqamAnalyzer::new(AnalyzerConfig {
                min_note_s,
                ..AnalyzerConfig::default()
            });
            let end = play(&mut a, 0.0, &melody);
            a.analyze(Some(&dataset()), Some(sp(Degree::Do, 0)), end)
        };
        let kept = run(0.0);
        let filtered = run(AnalyzerConfig::default().min_note_s);
        let a_of = |r: &MaqamAnalysis| {
            r.candidates
                .iter()
                .find(|c| c.maqam == "test-maqam-a")
                .unwrap()
                .clone()
        };
        assert!(!a_of(&kept).contradicting.is_empty());
        assert!(a_of(&filtered).contradicting.is_empty());
        assert_eq!(filtered.notes, kept.notes - 3);
    }

    #[test]
    fn old_notes_leave_the_window_but_still_set_the_tuning() {
        let run = |tuning_window_s: f64| {
            let mut a = MaqamAnalyzer::new(AnalyzerConfig {
                tuning_window_s,
                ..AnalyzerConfig::default()
            });
            let mut t = 0.0;
            for _ in 0..3 {
                t = play(&mut a, t, &detuned_melody(15.0));
            }
            // Twenty seconds later, a few notes: too little to tune from.
            let end = play(
                &mut a,
                t + 20.0,
                &[(60.15, 0.3), (62.15, 0.3), (65.15, 0.3)],
            );
            a.analyze(Some(&dataset()), None, end)
        };
        let r = run(60.0);
        assert_eq!(r.notes, 3, "only the last ten seconds are scored");
        assert!((r.tuning_offset_cents - 15.0).abs() < 1.0, "{r:#?}");
        assert_eq!(run(10.0).tuning_offset_cents, 0.0);
    }

    #[test]
    fn a_near_tie_keeps_the_current_maqam() {
        let mut ds = dataset();
        let mut twin = ds.maqamat[0].clone();
        twin.id = "test-maqam-a-twin".into();
        twin.related.clear();
        twin.modulations.clear();
        twin.ascending[6].cents = 1000.0;
        ds.maqamat.push(twin);
        let run = |continuity: f64| {
            let mut a = MaqamAnalyzer::new(AnalyzerConfig {
                weights: ScoreWeights {
                    continuity,
                    ..ScoreWeights::default()
                },
                ..AnalyzerConfig::default()
            });
            // Clearly maqam A (step 6 at 1050 cents) ...
            let t = play(&mut a, 0.0, &detuned_melody(0.0));
            let first = a.analyze(Some(&ds), Some(sp(Degree::Do, 0)), t);
            assert_eq!(first.candidates[0].maqam, "test-maqam-a");
            // ... then a passage whose step 6 (1020 cents) leans to the twin.
            let melody: Vec<(f64, f64)> = detuned_melody(0.0)
                .into_iter()
                .map(|(p, d)| if p == 70.5 { (70.2, 0.3) } else { (p, d) })
                .collect();
            let end = play(&mut a, t + 15.0, &melody);
            a.analyze(Some(&ds), Some(sp(Degree::Do, 0)), end)
                .candidates[0]
                .maqam
                .clone()
        };
        assert_eq!(run(0.0), "test-maqam-a-twin");
        assert_eq!(run(ScoreWeights::default().continuity), "test-maqam-a");
    }

    #[test]
    fn clear_forgets_the_current_maqam() {
        let mut a = analyzer();
        let end = play(&mut a, 0.0, &detuned_melody(0.0));
        a.analyze(Some(&dataset()), None, end);
        assert!(a.leader.is_some());
        a.clear();
        assert!(a.leader.is_none());
    }
}
