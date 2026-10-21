//! Print live notes from the default microphone (or `--simulate`).
//!
//! ```text
//! cargo run --release -p naghma-audio --example listen -- [seconds] [--simulate] [--transpose N]
//! ```
//!
//! Nothing is recorded or saved; only analysis results are printed.

use naghma_audio::capture::list_input_devices;
use naghma_audio::simulation::SimulationSpec;
use naghma_audio::{Engine, EngineConfig, EngineEvent, SourceConfig};
use naghma_core::interpret::PitchContext;
use naghma_core::notation::LabelOptions;
use naghma_core::pitch::{IntervalUnit, Transposition};
use naghma_core::tracking::LiveState;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let simulate = args.iter().any(|a| a == "--simulate");
    let transpose: f64 = args
        .iter()
        .position(|a| a == "--transpose")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    match list_input_devices() {
        Ok(devices) => {
            println!("input devices:");
            for d in devices {
                println!("  {} {}", if d.is_default { "*" } else { " " }, d.name);
            }
        }
        Err(e) => println!("cannot list devices: {e}"),
    }

    let source = if simulate {
        SourceConfig::Simulation(SimulationSpec::default())
    } else {
        SourceConfig::Microphone { device_id: None }
    };
    let config = EngineConfig {
        source,
        analysis: Default::default(),
        tracker: Default::default(),
        context: PitchContext {
            transposition: Transposition::new(transpose, IntervalUnit::Semitone),
            ..PitchContext::default()
        },
    };
    let labels = LabelOptions::default();
    let mut frames = 0u64;
    let mut peak_db = f64::NEG_INFINITY;
    let mut last_report = 0.0;
    let engine = match Engine::start(config, move |event| match event {
        EngineEvent::Frame(f) => {
            frames += 1;
            peak_db = peak_db.max(f.observation.level_db);
            if let Some(s) = &f.update.started {
                println!(
                    "{:7.2}s  {:<16} {:8.2} Hz  {:+6.1} c  (sounding {})",
                    s.start_s,
                    s.player.note.label(&labels).text,
                    s.frequency_hz,
                    s.player.deviation_cents,
                    s.sounding.note.label(&labels).text
                );
            }
            if f.observation.time_s - last_report >= 1.0 {
                last_report = f.observation.time_s;
                let state = match &f.update.state {
                    LiveState::Silent => "silent",
                    LiveState::Listening => "listening",
                    LiveState::Note(_) => "note",
                };
                println!(
                    "   [t={:5.1}s frames={frames} peak={peak_db:6.1} dBFS state={state} \
                     compute={:.0}µs latency={:?}ms]",
                    f.observation.time_s,
                    f.timing.compute_us,
                    f.timing.pipeline_latency_ms.map(|l| l.round())
                );
                peak_db = f64::NEG_INFINITY;
            }
        }
        EngineEvent::NoteEnded(_) => {}
        EngineEvent::Error { error } => eprintln!("engine error: {error}"),
    }) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cannot start: {e}");
            std::process::exit(1);
        }
    };
    let info = engine.info().clone();
    println!(
        "source: {:?} — {} @ {} Hz, {} ch, window {} hop {}",
        info.source, info.device_name, info.sample_rate, info.channels, info.window, info.hop
    );
    std::thread::sleep(Duration::from_secs_f64(seconds));
    let stats = engine.stats();
    engine.stop();
    println!("stats: {stats:?}");
}
