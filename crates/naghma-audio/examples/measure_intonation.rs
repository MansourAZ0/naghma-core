//! Measure where performers place each scale degree, per maqam, from
//! labelled recordings.
//!
//! ```text
//! cargo run --release -p naghma-audio --example measure_intonation -- manifest.json \
//!     [--json profiles.json] [--min-clips 3]
//! ```
//!
//! For every clip whose source states a maqam (the `eval_maqam` manifest
//! format), the full pipeline produces notes. The instrument's tuning offset
//! is estimated over the whole clip and removed. The clip's tonic is the one
//! under which the stated maqam fits best, searched over all 24 quarter-tone
//! classes with a wide (45-cent) tolerance so that the intonation being
//! measured does not decide the tonic. Each note is then assigned to the
//! nearest scale degree when it is clearly nearer to it than to any other,
//! and its deviation from the 24-EDO illustration is recorded, weighted by
//! duration (at most 2 s per note).
//!
//! Prints, per maqam and degree, the weighted median deviation, the
//! interquartile range, the number of clips and seconds of material.
//! `--json` writes one provisional [`IntonationProfile`] per maqam: the
//! measured tonic offset and each degree's measured position, with the
//! interquartile range as the accepted range. Degrees heard in fewer than
//! `--min-clips` clips keep their 24-EDO position.

use naghma_audio::{AnalysisConfig, analyze_samples};
use naghma_core::events::MusicalNoteEvent;
use naghma_core::interpret::PitchContext;
use naghma_core::maqam::analysis::{
    AnalyzerConfig, HistoryNote, MaqamAnalyzer, ScoreWeights, estimate_tuning_offset,
};
use naghma_core::maqam::{MaqamDataset, ScaleDegree, realize};
use naghma_core::notation::SpelledNote;
use naghma_core::tracking::TrackerConfig;
use naghma_core::tuning::{
    IntonationProfile, IntonationTarget, NoteSpelling, Provenance, ProvenanceStatus, SpellingTable,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, Deserialize)]
struct Manifest {
    clips: Vec<Clip>,
}

#[derive(Debug, Deserialize)]
struct Clip {
    id: String,
    path: PathBuf,
    maqam: Option<String>,
    #[serde(default = "yes")]
    include: bool,
}

fn yes() -> bool {
    true
}

fn read_wav(path: &Path) -> Result<(Vec<f32>, f64), String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 * scale)
                .collect()
        }
    };
    let mono = interleaved
        .chunks_exact(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();
    Ok((mono, spec.sample_rate as f64))
}

/// Degree identity within a maqam: scale step and its 24-EDO position.
type DegreeKey = (u8, i64);

#[derive(Debug, Default)]
struct Samples {
    /// (deviation in cents, weight).
    values: Vec<(f64, f64)>,
    clips: Vec<String>,
}

impl Samples {
    fn quantile(&self, q: f64) -> f64 {
        let mut v = self.values.clone();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        let total: f64 = v.iter().map(|x| x.1).sum();
        let mut acc = 0.0;
        for (x, w) in &v {
            acc += w;
            if acc >= q * total {
                return *x;
            }
        }
        v.last().map_or(0.0, |x| x.0)
    }

    fn seconds(&self) -> f64 {
        self.values.iter().map(|x| x.1).sum()
    }
}

/// Notes of a clip, with the tonic, illustration and offset that fit best.
struct Fitted {
    notes: Vec<HistoryNote>,
    offset: f64,
    tonic: NoteSpelling,
    variant: Option<String>,
    tonic_octave: Option<i8>,
}

fn fit_clip(path: &Path, maqam: &str, dataset: &MaqamDataset) -> Result<Fitted, String> {
    let (samples, sr) = read_wav(path)?;
    let frames = analyze_samples(
        &samples,
        sr,
        AnalysisConfig::default(),
        TrackerConfig::default(),
        PitchContext::default(),
    )
    .map_err(|e| e.to_string())?;
    let events: Vec<MusicalNoteEvent> = frames
        .iter()
        .filter_map(|f| f.update.ended.clone())
        .collect();
    let end = samples.len() as f64 / sr;
    let notes: Vec<HistoryNote> = events
        .iter()
        .map(|e| HistoryNote {
            start_s: e.start_s,
            duration_s: e.duration_s(end).max(0.0),
            pitch: e.player_pitch.0,
        })
        .filter(|n| n.duration_s >= 0.15)
        .collect();
    let offset = estimate_tuning_offset(&notes, 20.0).ok_or("too little material")?;
    let mut only = dataset.clone();
    only.maqamat.retain(|m| m.id == maqam);
    if only.maqamat.is_empty() {
        return Err(format!("maqam {maqam:?} not in the dataset"));
    }
    let mut analyzer = MaqamAnalyzer::new(AnalyzerConfig {
        window_s: 1e9,
        tuning_window_s: 1e9,
        min_notes: 1,
        min_distinct: 1,
        tolerance_cents: 45.0,
        tuning_offset_cents: Some(offset),
        min_score: f64::NEG_INFINITY,
        weights: ScoreWeights {
            continuity: 0.0,
            ..ScoreWeights::default()
        },
        ..AnalyzerConfig::default()
    });
    for n in &notes {
        analyzer.push(*n);
    }
    let table = SpellingTable::default_24();
    let best = (0..24)
        .filter_map(|k| {
            analyzer
                .analyze(Some(&only), Some(table.get(k)), end)
                .candidates
                .into_iter()
                .next()
        })
        .max_by(|a, b| a.score.total_cmp(&b.score))
        .ok_or("no candidate")?;
    Ok(Fitted {
        notes,
        offset,
        tonic: best.tonic,
        variant: best.variant,
        tonic_octave: best.tonic_octave,
    })
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(manifest_path) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: measure_intonation <manifest.json> [--json out.json] [--min-clips N]");
        return ExitCode::FAILURE;
    };
    let manifest: Manifest = match std::fs::read_to_string(manifest_path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cannot read {manifest_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let min_clips: usize = arg(&args, "--min-clips")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let base = Path::new(manifest_path)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let dataset = MaqamDataset::teaching_eight_24edo();
    let mut measured: BTreeMap<String, BTreeMap<DegreeKey, Samples>> = BTreeMap::new();
    let mut clip_count: BTreeMap<String, usize> = BTreeMap::new();

    for clip in manifest.clips.iter().filter(|c| c.include) {
        let Some(maqam_id) = &clip.maqam else {
            continue;
        };
        let path = if clip.path.is_absolute() {
            clip.path.clone()
        } else {
            base.join(&clip.path)
        };
        let fitted = match fit_clip(&path, maqam_id, &dataset) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("{}: {e}", clip.id);
                continue;
            }
        };
        let maqam = dataset.maqam(maqam_id).expect("checked in fit_clip");
        let scale = maqam
            .scale(fitted.variant.as_deref())
            .expect("variant from this maqam");
        let mut degrees = scale.to_vec();
        if fitted.variant.is_none() {
            degrees.extend(maqam.descending.iter().flatten().copied());
        }
        let repeating = maqam.octave_repeating;
        let tonic_cents = fitted.tonic_octave.map_or_else(
            || fitted.tonic.quarter_tone_class() as f64 * 50.0,
            |o| {
                SpelledNote::new(fitted.tonic.degree, fitted.tonic.accidental, o)
                    .nominal_pitch()
                    .0
                    * 100.0
            },
        );
        let positions: Vec<(u8, f64)> = degrees
            .iter()
            .map(|d| {
                (
                    d.step,
                    if repeating {
                        d.cents.rem_euclid(1200.0)
                    } else {
                        d.cents
                    },
                )
            })
            .collect();
        let distance = |a: f64, b: f64| {
            if repeating {
                let d = (a - b).rem_euclid(1200.0);
                if d > 600.0 { d - 1200.0 } else { d }
            } else {
                a - b
            }
        };
        let per_maqam = measured.entry(maqam_id.clone()).or_default();
        *clip_count.entry(maqam_id.clone()).or_default() += 1;
        println!(
            "# {:<22} {:<9} tonic {:?}{:+} variant {:<16} offset {:+5.1} c, {} notes",
            clip.id,
            maqam_id,
            fitted.tonic.degree,
            fitted.tonic.accidental.quarter_tones(),
            fitted.variant.as_deref().unwrap_or("primary"),
            fitted.offset,
            fitted.notes.len()
        );
        for n in &fitted.notes {
            let rel = n.pitch * 100.0 - fitted.offset - tonic_cents;
            let mut near: Vec<(f64, &(u8, f64))> =
                positions.iter().map(|p| (distance(rel, p.1), p)).collect();
            near.sort_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
            // Distinct degrees only (ascending and descending may coincide).
            near.dedup_by(|a, b| (a.1.1 - b.1.1).abs() < 1.0);
            let Some(&(dev, &(step, cents))) = near.first() else {
                continue;
            };
            let next = near.get(1).map_or(f64::INFINITY, |x| x.0.abs());
            if dev.abs() > 60.0 || next < dev.abs() + 50.0 {
                continue;
            }
            let s = per_maqam.entry((step, cents.round() as i64)).or_default();
            s.values.push((dev, n.duration_s.min(2.0)));
            if !s.clips.contains(&clip.id) {
                s.clips.push(clip.id.clone());
            }
        }
    }

    println!(
        "\n{:<9} {:>4} {:>6} {:>8} {:>15} {:>6} {:>6}",
        "maqam", "step", "24-EDO", "median", "IQR", "clips", "sec"
    );
    let mut profiles = Vec::new();
    for (maqam_id, degrees) in &measured {
        let maqam = dataset.maqam(maqam_id).expect("measured from this dataset");
        let shift = |s: &Samples| {
            if s.clips.len() >= min_clips {
                s.quantile(0.5)
            } else {
                0.0
            }
        };
        let tonic_shift = degrees
            .iter()
            .find(|(k, _)| k.1 == 0)
            .map_or(0.0, |(_, s)| shift(s));
        let mut targets = Vec::new();
        for (key, s) in degrees {
            let (q1, q2, q3) = (s.quantile(0.25), s.quantile(0.5), s.quantile(0.75));
            println!(
                "{:<9} {:>4} {:>6} {:>+8.1} {:>+7.1}..{:<+6.1} {:>6} {:>6.0}{}",
                maqam_id,
                key.0,
                key.1,
                q2,
                q1,
                q3,
                s.clips.len(),
                s.seconds(),
                if s.clips.len() < min_clips {
                    "  (too few clips: kept at 24-EDO)"
                } else {
                    ""
                }
            );
            let nominal = key.1 as f64;
            if !maqam.octave_repeating && nominal >= 1200.0 {
                continue; // a profile covers one octave above the tonic
            }
            let d = shift(s);
            let at = (nominal + d - tonic_shift).rem_euclid(1200.0);
            let spread = if s.clips.len() >= min_clips {
                Some((at + q1 - q2, at + q3 - q2))
            } else {
                None
            };
            let spelling = realize(
                maqam.default_tonic(4),
                &[ScaleDegree {
                    step: key.0,
                    cents: nominal,
                }],
            )[0]
            .note;
            targets.push(IntonationTarget {
                spellings: vec![NoteSpelling::new(spelling.degree, spelling.accidental)],
                cents_above_tonic: at,
                tolerance_cents: None,
                range_cents: spread.filter(|(lo, hi)| *lo <= at && at <= *hi),
            });
        }
        targets.sort_by(|a, b| a.cents_above_tonic.total_cmp(&b.cents_above_tonic));
        targets.dedup_by(|a, b| (a.cents_above_tonic - b.cents_above_tonic).abs() < 1.0);
        profiles.push(IntonationProfile {
            id: format!("{maqam_id}-measured-oud"),
            name: format!("{} as measured on recorded oud", maqam.name.latin),
            tonic: maqam.tonic,
            tonic_offset_cents: tonic_shift,
            targets,
            context: Some(maqam_id.clone()),
            provenance: Provenance {
                status: ProvenanceStatus::Provisional,
                source: format!(
                    "Measured with examples/measure_intonation.rs from {} public solo-oud \
                     recordings labelled {maqam_id} (docs/EVALUATION.md); weighted medians, \
                     not reviewed by a musician.",
                    clip_count.get(maqam_id).copied().unwrap_or(0)
                ),
                version: Some("2026-10".into()),
            },
        });
    }
    if let Some(out) = arg(&args, "--json") {
        for p in &profiles {
            if let Err(e) = p.validate() {
                eprintln!("{}: {e:?}", p.id);
                return ExitCode::FAILURE;
            }
        }
        if let Err(e) = std::fs::write(
            out,
            serde_json::to_string_pretty(&profiles).unwrap_or_default() + "\n",
        ) {
            eprintln!("{out}: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
