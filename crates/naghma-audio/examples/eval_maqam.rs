//! Evaluate maqam recognition against labelled recordings.
//!
//! ```text
//! cargo run --release -p naghma-audio --example eval_maqam -- manifest.json \
//!     [--interval 2] [--window 30] [--tolerance 30] [--config '{"minScore":0.5}'] \
//!     [--kind taqsim,doulab] [--only id1,id2] [--verbose] [--evidence] [--json report.json]
//! ```
//!
//! Every clip is transcribed with the full pipeline (YIN → note tracker),
//! then replayed into [`MaqamAnalyzer`] the way a live application does:
//! completed notes are pushed as they end and an analysis runs every
//! `--interval` seconds. Reported per clip and in aggregate:
//!
//! * **final top-1 / top-3** — the stated maqam at the end of the clip;
//! * **steps top-1** — share of analyses (with enough data) whose best
//!   candidate is the stated maqam, i.e. how often a live display is right;
//! * **tonic** — whether the candidate for the stated maqam has the stated
//!   tonic (only for clips whose source states one);
//! * **status** distribution (likely / ambiguous / unknown / insufficient);
//! * **offset** — estimated global tuning offset in cents;
//! * **first** — seconds until the best candidate was first correct.
//!
//! Labels come from the manifest only (what the source states). See
//! `docs/EVALUATION.md` for the manifest format.

use naghma_audio::{AnalysisConfig, analyze_samples};
use naghma_core::events::MusicalNoteEvent;
use naghma_core::interpret::PitchContext;
use naghma_core::maqam::MaqamDataset;
use naghma_core::maqam::analysis::{AnalysisStatus, AnalyzerConfig, MaqamAnalysis, MaqamAnalyzer};
use naghma_core::tracking::TrackerConfig;
use naghma_core::tuning::NoteSpelling;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    clips: Vec<Clip>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Clip {
    id: String,
    /// WAV path, relative to the manifest's directory or absolute.
    path: PathBuf,
    /// Maqam id as stated by the source; `None` for non-maqam material.
    maqam: Option<String>,
    /// Tonic as stated by the source, if it states one.
    #[serde(default)]
    tonic: Option<NoteSpelling>,
    #[serde(default)]
    kind: Option<String>,
    /// Excluded from evaluation when `false` (e.g. mixed-maqam material).
    #[serde(default = "yes")]
    include: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipResult {
    id: String,
    maqam: String,
    kind: String,
    final_top1: bool,
    final_top3: bool,
    final_status: Option<AnalysisStatus>,
    final_best: Option<String>,
    final_best_tonic: Option<NoteSpelling>,
    tonic_correct: Option<bool>,
    steps: usize,
    steps_top1: usize,
    steps_top3: usize,
    status_counts: BTreeMap<String, usize>,
    first_correct_s: Option<f64>,
    offset_cents: Option<f64>,
    notes: usize,
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
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

fn status_name(s: AnalysisStatus) -> &'static str {
    match s {
        AnalysisStatus::InsufficientData => "insufficient",
        AnalysisStatus::Unknown => "unknown",
        AnalysisStatus::Ambiguous => "ambiguous",
        AnalysisStatus::Likely => "likely",
    }
}

fn top_contains(a: &MaqamAnalysis, maqam: &str, k: usize) -> bool {
    a.candidates.iter().take(k).any(|c| c.maqam == maqam)
}

/// Duration-weighted circular mean of note deviations from the 24-EDO
/// grid (period 50 cents), in cents. Independent of the analyser so it can
/// be compared across analyser versions.
fn grid_offset(events: &[MusicalNoteEvent]) -> Option<f64> {
    let (mut s, mut c, mut w) = (0.0, 0.0, 0.0);
    for e in events {
        let d = e.duration_s(e.end_s.unwrap_or(e.start_s));
        if d < 0.1 {
            continue;
        }
        let phase = (e.sounding_pitch.0 * 100.0).rem_euclid(50.0) / 50.0 * std::f64::consts::TAU;
        s += d * phase.sin();
        c += d * phase.cos();
        w += d;
    }
    (w > 0.0).then(|| s.atan2(c) / std::f64::consts::TAU * 50.0)
}

/// Options shared by every clip.
#[derive(Debug, Clone, Copy)]
struct Run {
    config: AnalyzerConfig,
    tracker: TrackerConfig,
    interval: f64,
    verbose: bool,
    evidence: bool,
}

fn evaluate(
    clip: &Clip,
    path: &Path,
    dataset: &MaqamDataset,
    run: Run,
) -> Result<ClipResult, String> {
    let Run {
        config,
        tracker,
        interval,
        verbose,
        evidence,
    } = run;
    let maqam = clip.maqam.clone().unwrap_or_default();
    let (samples, sr) = read_wav(path)?;
    let frames = analyze_samples(
        &samples,
        sr,
        AnalysisConfig::default(),
        tracker,
        PitchContext::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut events: Vec<MusicalNoteEvent> = frames
        .iter()
        .filter_map(|f| f.update.ended.clone())
        .collect();
    events.sort_by(|a, b| a.end_s.unwrap_or(0.0).total_cmp(&b.end_s.unwrap_or(0.0)));
    let end = samples.len() as f64 / sr;

    let mut analyzer = MaqamAnalyzer::new(config);
    let mut result = ClipResult {
        id: clip.id.clone(),
        maqam: maqam.clone(),
        kind: clip.kind.clone().unwrap_or_default(),
        offset_cents: grid_offset(&events),
        notes: events.len(),
        ..ClipResult::default()
    };
    let mut next = 0;
    let mut t = interval;
    let mut last = None;
    while t <= end + 1e-9 {
        while next < events.len() && events[next].end_s.unwrap_or(0.0) <= t {
            let e = &events[next];
            analyzer.push_event(e, e.end_s.unwrap_or(t));
            next += 1;
        }
        let a = analyzer.analyze(Some(dataset), None, t);
        *result
            .status_counts
            .entry(status_name(a.status).into())
            .or_default() += 1;
        if a.status != AnalysisStatus::InsufficientData {
            result.steps += 1;
            if top_contains(&a, &maqam, 1) {
                result.steps_top1 += 1;
                result.first_correct_s.get_or_insert(t);
            }
            if top_contains(&a, &maqam, 3) {
                result.steps_top3 += 1;
            }
        }
        if verbose {
            let top: Vec<String> = a
                .candidates
                .iter()
                .take(3)
                .map(|c| {
                    format!(
                        "{}@{:?}{:+}:{:.2}",
                        c.maqam,
                        c.tonic.degree,
                        c.tonic.accidental.quarter_tones(),
                        c.score
                    )
                })
                .collect();
            println!(
                "    t={t:6.1} {:<12} {}",
                status_name(a.status),
                top.join("  ")
            );
            if evidence {
                for c in a.candidates.iter().take(3) {
                    let ev: Vec<String> = c
                        .supporting
                        .iter()
                        .chain(&c.contradicting)
                        .map(|e| serde_json::to_string(e).unwrap_or_default())
                        .collect();
                    println!("        {}: {}", c.maqam, ev.join(" "));
                }
            }
        }
        last = Some(a);
        t += interval;
    }
    if let Some(a) = last {
        result.final_top1 = top_contains(&a, &maqam, 1);
        result.final_top3 = top_contains(&a, &maqam, 3);
        result.final_status = Some(a.status);
        result.final_best = a.candidates.first().map(|c| c.maqam.clone());
        result.final_best_tonic = a.candidates.first().map(|c| c.tonic);
        if let Some(stated) = clip.tonic {
            result.tonic_correct = Some(
                a.candidates
                    .iter()
                    .find(|c| c.maqam == maqam)
                    .is_some_and(|c| c.tonic.quarter_tone_class() == stated.quarter_tone_class()),
            );
        }
    }
    Ok(result)
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "   -".into()
    } else {
        format!("{:3.0}%", 100.0 * n as f64 / d as f64)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(manifest_path) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!(
            "usage: eval_maqam <manifest.json> [--interval S] [--window S] [--tolerance C] \
             [--config JSON] [--tracker JSON] [--kind a,b] [--only id,..] [--verbose] [--evidence] [--json out]"
        );
        return ExitCode::FAILURE;
    };
    let manifest: Manifest = match std::fs::read_to_string(manifest_path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cannot read manifest {manifest_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let base = Path::new(manifest_path)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    // Analyzer configuration: defaults, then --config JSON, then shortcuts.
    let mut config_json = serde_json::to_value(AnalyzerConfig::default()).expect("serializable");
    if let Some(overrides) = arg(&args, "--config") {
        match serde_json::from_str::<serde_json::Value>(overrides) {
            Ok(serde_json::Value::Object(o)) => {
                for (k, v) in o {
                    config_json[k] = v;
                }
            }
            _ => {
                eprintln!("--config must be a JSON object");
                return ExitCode::FAILURE;
            }
        }
    }
    let mut config: AnalyzerConfig = match serde_json::from_value(config_json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("invalid --config: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(w) = arg(&args, "--window").and_then(|v| v.parse().ok()) {
        config.window_s = w;
    }
    if let Some(t) = arg(&args, "--tolerance").and_then(|v| v.parse().ok()) {
        config.tolerance_cents = t;
    }
    let tracker: TrackerConfig = match arg(&args, "--tracker") {
        Some(j) => match serde_json::from_str(j) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("invalid --tracker: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => TrackerConfig::default(),
    };
    let interval: f64 = arg(&args, "--interval")
        .and_then(|v| v.parse().ok())
        .unwrap_or(2.0);
    let kinds: Option<Vec<&str>> = arg(&args, "--kind").map(|k| k.split(',').collect());
    let only: Option<Vec<&str>> = arg(&args, "--only").map(|k| k.split(',').collect());
    let verbose = args.iter().any(|a| a == "--verbose");
    let evidence = args.iter().any(|a| a == "--evidence");
    let dataset = MaqamDataset::teaching_eight_24edo();

    println!(
        "# eval_maqam: window {:.0} s, interval {interval} s, tolerance {} c",
        config.window_s, config.tolerance_cents
    );
    println!(
        "{:<22} {:<9} {:<7} {:>4} {:>4} {:<10} {:<9} {:>5} {:>6} {:>6} {:>6} {:>5}",
        "clip",
        "stated",
        "kind",
        "top1",
        "top3",
        "status",
        "best",
        "tonic",
        "step1",
        "step3",
        "offset",
        "first"
    );
    let mut results = Vec::new();
    for clip in &manifest.clips {
        let Some(_) = clip.maqam.as_ref().filter(|_| clip.include) else {
            continue;
        };
        if kinds
            .as_ref()
            .is_some_and(|k| !k.contains(&clip.kind.as_deref().unwrap_or("")))
        {
            continue;
        }
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
        if verbose {
            println!("## {}", clip.id);
        }
        match evaluate(
            clip,
            &path,
            &dataset,
            Run {
                config,
                tracker,
                interval,
                verbose: verbose || evidence,
                evidence,
            },
        ) {
            Ok(r) => {
                println!(
                    "{:<22} {:<9} {:<7} {:>4} {:>4} {:<10} {:<9} {:>5} {:>6} {:>6} {:>6} {:>5}",
                    r.id,
                    r.maqam,
                    r.kind,
                    if r.final_top1 { "yes" } else { "-" },
                    if r.final_top3 { "yes" } else { "-" },
                    r.final_status.map_or("-", status_name),
                    r.final_best.as_deref().unwrap_or("-"),
                    match r.tonic_correct {
                        Some(true) => "yes",
                        Some(false) => "no",
                        None => "",
                    },
                    pct(r.steps_top1, r.steps),
                    pct(r.steps_top3, r.steps),
                    r.offset_cents.map_or("-".into(), |o| format!("{o:+.0}")),
                    r.first_correct_s.map_or("-".into(), |s| format!("{s:.0}")),
                );
                results.push(r);
            }
            Err(e) => eprintln!("{}: {e}", clip.id),
        }
    }

    // Aggregates.
    let n = results.len();
    let sum = |f: &dyn Fn(&ClipResult) -> bool| results.iter().filter(|r| f(r)).count();
    let steps: usize = results.iter().map(|r| r.steps).sum();
    let steps1: usize = results.iter().map(|r| r.steps_top1).sum();
    let steps3: usize = results.iter().map(|r| r.steps_top3).sum();
    let tonic_n = sum(&|r| r.tonic_correct.is_some());
    let tonic_ok = sum(&|r| r.tonic_correct == Some(true));
    println!("\n# per maqam: clips  final-top1  final-top3  steps-top1  steps-top3");
    let mut by: BTreeMap<&str, Vec<&ClipResult>> = BTreeMap::new();
    for r in &results {
        by.entry(r.maqam.as_str()).or_default().push(r);
    }
    for (m, rs) in &by {
        let st: usize = rs.iter().map(|r| r.steps).sum();
        println!(
            "{:<10} {:>3}   {:>6}   {:>6}   {:>6}   {:>6}",
            m,
            rs.len(),
            pct(rs.iter().filter(|r| r.final_top1).count(), rs.len()),
            pct(rs.iter().filter(|r| r.final_top3).count(), rs.len()),
            pct(rs.iter().map(|r| r.steps_top1).sum(), st),
            pct(rs.iter().map(|r| r.steps_top3).sum(), st),
        );
    }
    let mut statuses: BTreeMap<String, usize> = BTreeMap::new();
    for r in &results {
        for (k, v) in &r.status_counts {
            *statuses.entry(k.clone()).or_default() += v;
        }
    }
    let total_steps: usize = statuses.values().sum();
    let mut confusion: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in &results {
        *confusion
            .entry((r.maqam.clone(), r.final_best.clone().unwrap_or("-".into())))
            .or_default() += 1;
    }
    let first: Vec<f64> = results.iter().filter_map(|r| r.first_correct_s).collect();
    let mut first_sorted = first.clone();
    first_sorted.sort_by(f64::total_cmp);
    println!(
        "\n# TOTAL clips {n}: final top-1 {} ({}/{n}), top-3 {}, steps top-1 {}, steps top-3 {}, tonic {} ({tonic_ok}/{tonic_n})",
        pct(sum(&|r| r.final_top1), n),
        sum(&|r| r.final_top1),
        pct(sum(&|r| r.final_top3), n),
        pct(steps1, steps),
        pct(steps3, steps),
        pct(tonic_ok, tonic_n),
    );
    println!(
        "# status share over all analyses: {}",
        statuses
            .iter()
            .map(|(k, v)| format!("{k} {}", pct(*v, total_steps).trim()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "# time to first correct: median {} s over {} clips (never correct: {})",
        first_sorted
            .get(first_sorted.len() / 2)
            .map_or("-".into(), |s| format!("{s:.0}")),
        first.len(),
        n - first.len()
    );
    println!(
        "# confusions (stated -> final best): {}",
        confusion
            .iter()
            .filter(|((a, b), _)| a != b)
            .map(|((a, b), c)| format!("{a}->{b} x{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if let Some(out) = arg(&args, "--json") {
        let json = serde_json::to_string_pretty(&results).expect("serializable");
        if let Err(e) = std::fs::write(out, json) {
            eprintln!("cannot write {out}: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
