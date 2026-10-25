//! Regression expectations independently checked against the cited notation.
use naghma_core::maqam::analysis::{AnalyzerConfig, Evidence, HistoryNote, MaqamAnalyzer};
use naghma_core::maqam::{MaqamDataset, SCHEMA_VERSION};
use naghma_core::notation::SpelledNote;
use naghma_core::pitch::Pitch;
use naghma_core::tuning::{NoteSpelling, ProvenanceStatus, SpellingTable};

fn note(s: &str) -> SpelledNote {
    s.parse().unwrap()
}
fn names(id: &str, tonic: &str, variant: Option<&str>) -> Vec<String> {
    MaqamDataset::teaching_eight_24edo()
        .maqam(id)
        .unwrap()
        .realize_variant(note(tonic), variant)
        .unwrap()
        .iter()
        .map(|n| n.note.to_string())
        .collect()
}

#[test]
fn dataset_is_sourced_provisional_and_has_exactly_the_eight_requested_maqamat() {
    let ds = MaqamDataset::teaching_eight_24edo();
    assert_eq!(ds.schema_version, SCHEMA_VERSION);
    assert_eq!(ds.provenance.status, ProvenanceStatus::Provisional);
    assert_eq!(
        ds.maqamat.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        [
            "saba", "nahawand", "ajam", "bayati", "sikah", "hijaz", "rast", "kurd"
        ]
    );
    assert!(
        ds.maqamat
            .iter()
            .all(|m| !m.sources.is_empty() && m.phrases.is_empty())
    );
    assert_eq!(ds.jins("upper-rast").unwrap().tonic_step, 3);
    assert_eq!(ds.jins("upper-ajam").unwrap().tonic_step, 3);
    let mut invalid = ds.clone();
    invalid.schema_version = 1;
    assert!(
        invalid.validate().is_err(),
        "old readers must not silently repeat Saba"
    );
    invalid = ds.clone();
    invalid.maqamat[0].variants[0].ajnas[0].jins = "missing".into();
    assert!(invalid.validate().is_err());
    invalid = ds.clone();
    invalid.ajnas[0].tonic_step = 99;
    assert!(invalid.validate().is_err());
    assert!(ds.maqam("bayati").unwrap().scale(Some("missing")).is_err());
}

#[test]
fn bayati_re_sol_la_spellings_and_upper_variants() {
    assert_eq!(
        names("bayati", "Re3", None),
        [
            "Re3",
            "Mi half-flat 3",
            "Fa3",
            "Sol3",
            "La3",
            "Si flat 3",
            "Do4",
            "Re4"
        ]
    );
    assert_eq!(
        names("bayati", "Sol3", None),
        [
            "Sol3",
            "La half-flat 3",
            "Si flat 3",
            "Do4",
            "Re4",
            "Mi flat 4",
            "Fa4",
            "Sol4"
        ]
    );
    assert_eq!(
        names("bayati", "La3", None),
        [
            "La3",
            "Si half-flat 3",
            "Do4",
            "Re4",
            "Mi4",
            "Fa4",
            "Sol4",
            "La4"
        ]
    );
    assert_eq!(
        names("bayati", "La3", Some("rast-upper"))[5],
        "Fa half-sharp 4"
    );
    assert_eq!(
        names("bayati", "Sol3", Some("rast-upper"))[5],
        "Mi half-flat 4"
    );
}

#[test]
fn canonical_reference_spellings_match_the_source_scores() {
    assert_eq!(
        names("nahawand", "Do3", None),
        [
            "Do3",
            "Re3",
            "Mi flat 3",
            "Fa3",
            "Sol3",
            "La flat 3",
            "Si3",
            "Do4"
        ]
    );
    assert_eq!(names("nahawand", "Do3", Some("kurd-upper"))[6], "Si flat 3");
    assert_eq!(
        names("ajam", "Do3", None),
        ["Do3", "Re3", "Mi3", "Fa3", "Sol3", "La3", "Si3", "Do4"]
    );
    assert_eq!(
        names("hijaz", "Re3", None),
        [
            "Re3",
            "Mi flat 3",
            "Fa sharp 3",
            "Sol3",
            "La3",
            "Si flat 3",
            "Do4",
            "Re4"
        ]
    );
    assert_eq!(
        names("rast", "Do3", None),
        [
            "Do3",
            "Re3",
            "Mi half-flat 3",
            "Fa3",
            "Sol3",
            "La3",
            "Si half-flat 3",
            "Do4"
        ]
    );
    assert_eq!(
        names("kurd", "Re3", None),
        [
            "Re3",
            "Mi flat 3",
            "Fa3",
            "Sol3",
            "La3",
            "Si flat 3",
            "Do4",
            "Re4"
        ]
    );
    assert_eq!(
        names("sikah", "Mi half-flat 3", None),
        [
            "Mi half-flat 3",
            "Fa3",
            "Sol3",
            "La3",
            "Si half-flat 3",
            "Do4",
            "Re4",
            "Mi half-flat 4"
        ]
    );
}

#[test]
fn every_24_edo_tonic_preserves_all_documented_intervals_and_spelling() {
    let ds = MaqamDataset::teaching_eight_24edo();
    for maqam in &ds.maqamat {
        for variant in
            std::iter::once(None).chain(maqam.variants.iter().map(|v| Some(v.id.as_str())))
        {
            for i in 0..24 {
                let sp = SpellingTable::default_24().get(i);
                let tonic = SpelledNote::new(sp.degree, sp.accidental, 3);
                let realized = maqam.realize_variant(tonic, variant).unwrap();
                for (r, d) in realized.iter().zip(maqam.scale(variant).unwrap()) {
                    assert!((r.pitch.0 - tonic.nominal_pitch().0 - d.cents / 100.0).abs() < 1e-8);
                    assert!(
                        (r.note.nominal_pitch().0 - r.pitch.0).abs() < 1e-8,
                        "{} {:?} on {}: {}",
                        maqam.id,
                        variant,
                        tonic,
                        r.note
                    );
                }
            }
        }
    }
}

#[test]
fn saba_keeps_register_specific_notes_and_separate_eighth_degrees() {
    let ds = MaqamDataset::teaching_eight_24edo();
    let saba = ds.maqam("saba").unwrap();
    let targets = saba
        .targets_between(note("Re3"), None, Pitch(36.0), Pitch(84.0))
        .unwrap();
    assert_eq!(targets.len(), 10);
    assert_eq!(targets[7].note, note("Re flat 4"));
    assert!(targets.iter().any(|n| n.note == note("Mi4")));
    assert!(!targets.iter().any(|n| n.note == note("Mi half-flat 4")));
    assert!(!targets.iter().any(|n| n.note == note("Re2")));
    assert_eq!(names("saba", "Re3", Some("ajam-upper"))[7], "Re4");
    assert!(
        ds.maqam("bayati")
            .unwrap()
            .targets_between(note("Re3"), None, Pitch(36.0), Pitch(84.0))
            .unwrap()
            .len()
            > 10
    );
}

fn analyze(
    id: &str,
    offsets: &[f64],
    root: SpelledNote,
) -> naghma_core::maqam::analysis::MaqamCandidate {
    let mut ds = MaqamDataset::teaching_eight_24edo();
    ds.maqamat.retain(|m| m.id == id);
    let mut a = MaqamAnalyzer::new(AnalyzerConfig {
        tolerance_cents: 20.0,
        ..Default::default()
    });
    for (i, offset) in offsets.iter().enumerate() {
        a.push(HistoryNote {
            start_s: i as f64 * 0.5,
            duration_s: 0.5,
            pitch: root.nominal_pitch().0 + offset / 100.0,
        });
    }
    a.analyze(
        Some(&ds),
        Some(NoteSpelling::new(root.degree, root.accidental)),
        offsets.len() as f64 * 0.5,
    )
    .candidates
    .remove(0)
}

#[test]
fn recognition_selects_an_alternative_without_unioning_its_notes() {
    let played = [0.0, 150.0, 300.0, 500.0, 700.0, 850.0, 1000.0, 1200.0];
    let candidate = analyze("bayati", &played, note("La3"));
    assert_eq!(candidate.variant.as_deref(), Some("rast-upper"));
    let mixed = analyze(
        "bayati",
        &[
            0.0, 150.0, 300.0, 500.0, 700.0, 800.0, 850.0, 1000.0, 1200.0,
        ],
        note("La3"),
    );
    assert!(
        mixed
            .contradicting
            .iter()
            .any(|e| matches!(e, Evidence::ForeignNotes { share, .. } if *share > 0.0))
    );
}

#[test]
fn nonperiodic_recognition_reports_an_anchor_and_rejects_octave_folding() {
    let candidate = analyze(
        "saba",
        &[
            0.0, 150.0, 300.0, 400.0, 700.0, 800.0, 1000.0, 1100.0, 1400.0, 1500.0,
        ],
        note("Re3"),
    );
    assert_eq!(candidate.tonic_octave, Some(3));
    assert!(candidate.contradicting.is_empty());
    let wrong = analyze(
        "saba",
        &[
            0.0, 150.0, 300.0, 400.0, 700.0, 800.0, 1000.0, 1100.0, 1350.0, 1500.0,
        ],
        note("Re3"),
    );
    assert!(
        wrong
            .contradicting
            .iter()
            .any(|e| matches!(e, Evidence::ForeignNotes { share, .. } if *share > 0.0))
    );
}

#[test]
fn notes_below_a_nonperiodic_register_compare_with_the_documented_octave() {
    // Saba on Re3 with excursions to Do3 and La2 below the tonic (common on
    // the oud): they are compared an octave up (Do4, La3 are Saba degrees),
    // not reported as foreign. Notes inside the register stay exact.
    let candidate = analyze(
        "saba",
        &[
            0.0, 150.0, 300.0, 400.0, 300.0, 150.0, 0.0, -200.0, -500.0, -200.0, 0.0,
        ],
        note("Re3"),
    );
    assert_eq!(candidate.tonic_octave, Some(3));
    assert!(candidate.contradicting.is_empty(), "{candidate:#?}");
}
