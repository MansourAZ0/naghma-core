//! Print the notes the pipeline tracks in a WAV file: start, duration,
//! sounding pitch (semitones) and confidence, one per line.
//!
//! ```text
//! cargo run --release -p naghma-audio --example dump_notes -- clip.wav [--frames]
//! ```
//!
//! `--frames` prints, per second, how many analysis frames were voiced,
//! aperiodic, silent or out of range, with the median level and confidence:
//! where and why the detector hears nothing.

use naghma_audio::{AnalysisConfig, analyze_samples};
use naghma_core::interpret::PitchContext;
use naghma_core::tracking::TrackerConfig;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: dump_notes <clip.wav>");
        return ExitCode::FAILURE;
    };
    let mut reader = match hound::WavReader::open(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
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
    let mono: Vec<f32> = interleaved
        .chunks_exact(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();
    let frames = match analyze_samples(
        &mono,
        f64::from(spec.sample_rate),
        AnalysisConfig::default(),
        TrackerConfig::default(),
        PitchContext::default(),
    ) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if std::env::args().any(|a| a == "--frames") {
        println!("# second voiced aperiodic silence other level_db confidence");
        let mut second = 0usize;
        let mut rows: Vec<&naghma_audio::AnalysisFrame> = Vec::new();
        let flush = |second: usize, rows: &mut Vec<&naghma_audio::AnalysisFrame>| {
            if rows.is_empty() {
                return;
            }
            use naghma_core::events::ObservationStatus as S;
            let count = |s: S| rows.iter().filter(|f| f.observation.status == s).count();
            let median = |mut v: Vec<f64>| {
                v.sort_by(f64::total_cmp);
                v[v.len() / 2]
            };
            let voiced = count(S::Voiced);
            let aperiodic = count(S::Aperiodic);
            let silence = count(S::Silence);
            println!(
                "{second:4} {voiced:4} {aperiodic:4} {silence:4} {:4} {:6.1} {:.2}",
                rows.len() - voiced - aperiodic - silence,
                median(rows.iter().map(|f| f.observation.level_db).collect()),
                median(rows.iter().map(|f| f.observation.confidence).collect()),
            );
            rows.clear();
        };
        for f in &frames {
            let s = f.observation.time_s.max(0.0) as usize;
            if s != second {
                flush(second, &mut rows);
                second = s;
            }
            rows.push(f);
        }
        flush(second, &mut rows);
        return ExitCode::SUCCESS;
    }
    println!("# start_s duration_s pitch confidence");
    for e in frames.iter().filter_map(|f| f.update.ended.as_ref()) {
        println!(
            "{:8.3} {:6.3} {:7.2} {:.2}",
            e.start_s,
            e.duration_s(e.end_s.unwrap_or(e.start_s)),
            e.sounding_pitch.0,
            e.confidence
        );
    }
    ExitCode::SUCCESS
}
