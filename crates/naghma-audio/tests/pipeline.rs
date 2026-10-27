//! End-to-end: synthetic plucked notes → Analyzer → note events.
//!
//! Synthetic signals only; this does not establish accuracy on real oud
//! recordings.

use naghma_audio::analysis::{AnalysisConfig, AnalysisFrame, analyze_samples};
use naghma_core::interpret::PitchContext;
use naghma_core::notation::SpelledNote;
use naghma_core::pitch::{IntervalUnit, ReferencePitch, Transposition};
use naghma_core::tracking::TrackerConfig;
use pitchforge::signals::{self, PluckParams};

const SR: f64 = 48_000.0;

fn note(s: &str) -> SpelledNote {
    s.parse().unwrap()
}

fn pluck_params() -> PluckParams {
    PluckParams {
        pick_noise: 0.4,
        t60: 1.2,
        ..PluckParams::default()
    }
}

/// Render a melody given as (player note, seconds) under `transposition`.
fn render(melody: &[(&str, f64)], transposition: Transposition) -> (Vec<f32>, Vec<f64>) {
    let reference = ReferencePitch::default();
    let mut out = vec![0.0f32; (0.2 * SR) as usize];
    let mut onsets = Vec::new();
    for (name, dur) in melody {
        let sounding = transposition.player_to_sounding(note(name).nominal_pitch());
        onsets.push(out.len() as f64 / SR);
        let mut tone = signals::pluck(
            sounding.to_frequency(reference),
            SR,
            (dur * SR) as usize,
            0.6,
            pluck_params(),
        );
        signals::add_noise(&mut tone, 40.0, onsets.len() as u64);
        out.extend_from_slice(&tone);
    }
    out.extend(std::iter::repeat_n(0.0, (0.3 * SR) as usize));
    (out, onsets)
}

fn run(samples: &[f32], context: PitchContext) -> Vec<AnalysisFrame> {
    analyze_samples(
        samples,
        SR,
        AnalysisConfig::default(),
        TrackerConfig::default(),
        context,
    )
    .unwrap()
}

fn started_player_notes(frames: &[AnalysisFrame]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|f| f.update.started.as_ref())
        .map(|e| e.player.note.to_string())
        .collect()
}

#[test]
fn quarter_tone_melody_is_transcribed() {
    let melody = [
        ("Re3", 0.4),
        ("Mi half-flat 3", 0.4),
        ("Fa3", 0.4),
        ("Sol3", 0.4),
        ("La3", 0.4),
        ("Si half-flat 3", 0.4),
        ("Do4", 0.4),
        ("Re4", 0.4),
    ];
    let (samples, _) = render(&melody, Transposition::NONE);
    let frames = run(&samples, PitchContext::default());
    let names = started_player_notes(&frames);
    let expected: Vec<String> = melody.iter().map(|(n, _)| note(n).to_string()).collect();
    assert_eq!(names, expected);
}

#[test]
fn transposed_instrument_shows_player_names() {
    // The player fingers this melody on an oud tuned 2 semitones low.
    let melody = [("Re4", 0.4), ("Mi half-flat 4", 0.4), ("Do4", 0.4)];
    let t = Transposition::new(-2.0, IntervalUnit::Semitone);
    let (samples, _) = render(&melody, t);
    let ctx = PitchContext {
        transposition: t,
        ..PitchContext::default()
    };
    let frames = run(&samples, ctx);
    assert_eq!(
        started_player_notes(&frames),
        ["Re4", "Mi half-flat 4", "Do4"]
    );
    let sounding: Vec<String> = frames
        .iter()
        .filter_map(|f| f.update.started.as_ref())
        .map(|e| e.sounding.note.to_string())
        .collect();
    assert_eq!(sounding, ["Do4", "Re half-flat 4", "Si flat 3"]);
}

#[test]
fn tremolo_gives_one_event_per_pluck() {
    let melody: Vec<(&str, f64)> = (0..6).map(|_| ("Sol3", 0.125)).collect();
    let (samples, _) = render(&melody, Transposition::NONE);
    let frames = run(&samples, PitchContext::default());
    let names = started_player_notes(&frames);
    assert_eq!(names.len(), 6, "{names:?}");
    assert!(names.iter().all(|n| n == "Sol3"));
}

#[test]
fn silence_and_noise_produce_no_notes() {
    let silence = vec![0.0f32; SR as usize];
    assert!(started_player_notes(&run(&silence, PitchContext::default())).is_empty());
    let noise = signals::white_noise(SR as usize, 0.05, 11);
    assert!(started_player_notes(&run(&noise, PitchContext::default())).is_empty());
}

#[test]
fn nonfinite_input_is_reported_and_analysis_recovers() {
    use naghma_core::events::ObservationStatus;
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut samples = signals::sine(220.0, SR, SR as usize, 0.5, 0.0);
        samples[0] = bad;
        let frames = run(&samples, PitchContext::default());
        let first = &frames[0];
        assert_eq!(first.observation.status, ObservationStatus::InvalidInput);
        assert_eq!(first.observation.frequency_hz, None);
        assert_eq!(first.observation.confidence, 0.0);
        assert!(first.onset_strength_db.is_none());
        assert!(frames.last().unwrap().observation.frequency_hz.is_some());
        assert!(frames.iter().all(|f| f.observation.level_db.is_finite()));
    }
}

#[test]
fn onset_to_display_latency() {
    let melody = [("La2", 0.6), ("Re3", 0.6), ("Sol3", 0.6), ("Do4", 0.6)];
    let (samples, onsets) = render(&melody, Transposition::NONE);
    let frames = run(&samples, PitchContext::default());
    let mut latencies = Vec::new();
    let mut k = 0;
    for f in &frames {
        if f.update.started.is_some() {
            // Time at which this frame could be shown = end of its window.
            let shown_at = f.end_sample as f64 / SR;
            latencies.push((shown_at - onsets[k]) * 1000.0);
            k += 1;
        }
    }
    assert_eq!(latencies.len(), melody.len());
    println!("onset → note shown (algorithmic, excl. device buffering): {latencies:.1?} ms");
    for l in &latencies {
        assert!(*l < 120.0, "latency {l} ms");
    }
}
