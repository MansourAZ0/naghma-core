//! Measure note-tracking artefacts on real recordings, without note labels.
//!
//! ```text
//! cargo run --release -p naghma-audio --example tracker_artifacts -- manifest.json \
//!     [--tracker '{"settleS":0}'] [--analysis '{"hop":256}'] [--only id1,id2] [--json out.json]
//! ```
//!
//! Every clip in the manifest (the format of `eval_maqam`) runs through the
//! full pipeline. These patterns are artefacts, not music, so they can be
//! counted on any recording:
//!
//! * **split** — a note started by a pluck that lasts under 150 ms and is
//!   followed at once, with no new pluck, by a note less than a semitone
//!   away: the pluck's transient was reported as a separate note.
//! * **octave** — a note followed at once, with no new pluck, by one an
//!   octave, a twelfth or two octaves away: a harmonic was taken for the
//!   fundamental (a fretted oud note cannot jump an octave without a pluck).
//! * **wrong name** — share of the time a note is on screen while it shows a
//!   name other than the one the finished note ends up with (a pluck's
//!   first reading, before the tracker renames the note in place).
//! * **latency** — from the detected attack to the first frame that shows
//!   the note (algorithmic only: device buffering is not included).

use naghma_audio::{AnalysisConfig, analyze_samples};
use naghma_core::events::{MusicalNoteEvent, NoteStart};
use naghma_core::interpret::PitchContext;
use naghma_core::tracking::{LiveState, TrackerConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipResult {
    id: String,
    minutes: f64,
    notes: usize,
    splits: usize,
    octaves: usize,
    shown_s: f64,
    wrong_name_s: f64,
    latencies_ms: Vec<f64>,
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

/// `true` if `cents` is within 60 cents of an octave, a twelfth or two octaves.
fn harmonic_jump(cents: f64) -> bool {
    [1200.0, 1902.0, 2400.0]
        .iter()
        .any(|h| (cents.abs() - h).abs() < 60.0)
}

fn evaluate(
    path: &Path,
    id: &str,
    analysis: AnalysisConfig,
    tracker: TrackerConfig,
) -> Result<ClipResult, String> {
    let (samples, sr) = read_wav(path)?;
    let frames = analyze_samples(&samples, sr, analysis, tracker, PitchContext::default())
        .map_err(|e| e.to_string())?;
    let mut events: Vec<MusicalNoteEvent> = frames
        .iter()
        .filter_map(|f| f.update.ended.clone())
        .collect();
    events.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    let by_id: HashMap<u64, &MusicalNoteEvent> = events.iter().map(|e| (e.id, e)).collect();

    let mut r = ClipResult {
        id: id.to_owned(),
        minutes: samples.len() as f64 / sr / 60.0,
        notes: events.len(),
        ..ClipResult::default()
    };
    for pair in events.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let gap = b.start_s - a.end_s.unwrap_or(a.start_s);
        if b.start_kind != NoteStart::PitchChange || gap > 0.03 {
            continue;
        }
        let jump = (b.sounding_pitch.0 - a.sounding_pitch.0) * 100.0;
        let duration = a.end_s.unwrap_or(a.start_s) - a.start_s;
        if a.start_kind == NoteStart::Attack && duration < 0.15 && jump.abs() < 100.0 {
            r.splits += 1;
        }
        if harmonic_jump(jump) {
            r.octaves += 1;
        }
    }
    let hop_s = frames
        .windows(2)
        .map(|w| (w[1].end_sample - w[0].end_sample) as f64 / sr)
        .next()
        .unwrap_or(0.0);
    for f in &frames {
        if let Some(started) = &f.update.started {
            if started.start_kind == NoteStart::Attack {
                r.latencies_ms
                    .push((f.end_sample as f64 / sr - started.start_s) * 1000.0);
            }
        }
        if let LiveState::Note(live) = &f.update.state {
            if let Some(event) = by_id.get(&live.event_id) {
                r.shown_s += hop_s;
                if live.reading.player.note != event.player.note {
                    r.wrong_name_s += hop_s;
                }
            }
        }
    }
    Ok(r)
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(manifest_path) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!(
            "usage: tracker_artifacts <manifest.json> [--tracker JSON] [--analysis JSON] \
             [--only id1,id2] [--json out.json]"
        );
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
    let tracker: TrackerConfig = match arg(&args, "--tracker").map(serde_json::from_str) {
        None => TrackerConfig::default(),
        Some(Ok(t)) => t,
        Some(Err(e)) => {
            eprintln!("--tracker: {e}");
            return ExitCode::FAILURE;
        }
    };
    let analysis: AnalysisConfig = match arg(&args, "--analysis").map(serde_json::from_str) {
        None => AnalysisConfig::default(),
        Some(Ok(a)) => a,
        Some(Err(e)) => {
            eprintln!("--analysis: {e}");
            return ExitCode::FAILURE;
        }
    };
    let only: Option<Vec<&str>> = arg(&args, "--only").map(|s| s.split(',').collect());
    let base = Path::new(manifest_path)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    println!(
        "{:<24} {:>6} {:>7} {:>8} {:>8} {:>7} {:>8}",
        "clip", "notes", "/min", "split/m", "octav/m", "wrong", "lat ms"
    );
    let mut results = Vec::new();
    for clip in &manifest.clips {
        if only
            .as_ref()
            .is_some_and(|o| !o.contains(&clip.id.as_str()))
        {
            continue;
        }
        let path = if clip.path.is_absolute() {
            clip.path.clone()
        } else {
            base.join(&clip.path)
        };
        match evaluate(&path, &clip.id, analysis, tracker) {
            Ok(mut r) => {
                let lat = median(&mut r.latencies_ms.clone());
                println!(
                    "{:<24} {:>6} {:>7.0} {:>8.1} {:>8.1} {:>6.1}% {:>8.0}",
                    r.id,
                    r.notes,
                    r.notes as f64 / r.minutes,
                    r.splits as f64 / r.minutes,
                    r.octaves as f64 / r.minutes,
                    100.0 * r.wrong_name_s / r.shown_s.max(1e-9),
                    lat
                );
                r.latencies_ms.shrink_to_fit();
                results.push(r);
            }
            Err(e) => eprintln!("{}: {e}", clip.id),
        }
    }
    let minutes: f64 = results.iter().map(|r| r.minutes).sum();
    let sum = |f: fn(&ClipResult) -> f64| results.iter().map(f).sum::<f64>();
    let mut latencies: Vec<f64> = results
        .iter()
        .flat_map(|r| r.latencies_ms.iter().copied())
        .collect();
    let p90 = {
        latencies.sort_by(f64::total_cmp);
        latencies
            .get(latencies.len() * 9 / 10)
            .copied()
            .unwrap_or(f64::NAN)
    };
    println!(
        "# TOTAL {} clips, {:.1} min: notes/min {:.0}, splits/min {:.2}, octave flips/min {:.2}, \
         wrong name {:.2}% of shown time, latency median {:.0} ms (p90 {:.0} ms)",
        results.len(),
        minutes,
        sum(|r| r.notes as f64) / minutes,
        sum(|r| r.splits as f64) / minutes,
        sum(|r| r.octaves as f64) / minutes,
        100.0 * sum(|r| r.wrong_name_s) / sum(|r| r.shown_s).max(1e-9),
        median(&mut latencies),
        p90
    );
    if let Some(out) = arg(&args, "--json") {
        if let Err(e) = std::fs::write(
            out,
            serde_json::to_string_pretty(&results).unwrap_or_default(),
        ) {
            eprintln!("{out}: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
