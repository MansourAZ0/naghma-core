//! Transcribe a WAV recording into note events with the full pipeline
//! (YIN → note tracker → solfège), for validating against real recordings.
//!
//! ```text
//! cargo run --release -p naghma-audio --example analyze_wav -- recording.wav \
//!     [--transpose -2] [--reference 440] [--script arabic] [--frames]
//! ```
//!
//! Prints one line per detected note: start/end time, player note, sounding
//! note, median frequency, deviation, confidence. `--frames` also prints
//! every analysis frame. Multi-channel files are mixed to mono.

use naghma_audio::{AnalysisConfig, analyze_samples};
use naghma_core::interpret::PitchContext;
use naghma_core::notation::{AccidentalStyle, LabelOptions, Script};
use naghma_core::pitch::{IntervalUnit, ReferencePitch, Transposition};
use naghma_core::tracking::TrackerConfig;
use std::process::ExitCode;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!(
            "usage: analyze_wav <file.wav> [--transpose N] [--reference HZ] [--script arabic] [--frames]"
        );
        return ExitCode::FAILURE;
    };
    let transpose: f64 = arg(&args, "--transpose").unwrap_or(0.0);
    let reference: f64 = arg(&args, "--reference").unwrap_or(440.0);
    let script = if args.iter().any(|a| a == "arabic") {
        Script::Arabic
    } else {
        Script::Latin
    };
    let show_frames = args.iter().any(|a| a == "--frames");

    let mut reader = match hound::WavReader::open(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot open {path}: {e}");
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
    let sr = spec.sample_rate as f64;

    let context = PitchContext {
        reference: ReferencePitch::new(reference).unwrap_or_default(),
        transposition: Transposition::new(transpose, IntervalUnit::Semitone),
        ..PitchContext::default()
    };
    let labels = LabelOptions {
        script,
        accidentals: AccidentalStyle::Words,
        ..LabelOptions::default()
    };
    let frames = match analyze_samples(
        &mono,
        sr,
        AnalysisConfig::default(),
        TrackerConfig::default(),
        context,
    ) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("invalid configuration: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "# {path}: {:.1} s, {} Hz, {ch} ch | transposition {transpose:+} semitones, La4 = {reference} Hz",
        mono.len() as f64 / sr,
        spec.sample_rate
    );
    if show_frames {
        println!("# frames: time_s  status  f0_hz  confidence  level_db  onset");
        for f in &frames {
            let o = &f.observation;
            println!(
                "{:8.3}  {:<10?} {:>9}  {:.3}  {:6.1}  {}",
                o.time_s,
                o.status,
                o.frequency_hz.map_or("-".into(), |h| format!("{h:.2}")),
                o.confidence,
                o.level_db,
                if f.update.onset { "ONSET" } else { "" }
            );
        }
    }
    println!(
        "# notes: start_s  end_s  player_note  sounding_note  median_hz  cents  confidence  start"
    );
    let mut count = 0;
    for e in frames.iter().filter_map(|f| f.update.ended.as_ref()) {
        count += 1;
        println!(
            "{:8.3} {:8.3}  {:<22} {:<22} {:9.2}  {:+6.1}  {:.3}  {:?}",
            e.start_s,
            e.end_s.unwrap_or(f64::NAN),
            e.player.note.label(&labels).text,
            e.sounding.note.label(&labels).text,
            e.frequency_hz,
            e.player.deviation_cents,
            e.confidence,
            e.start_kind
        );
    }
    println!("# {count} completed notes");
    ExitCode::SUCCESS
}
