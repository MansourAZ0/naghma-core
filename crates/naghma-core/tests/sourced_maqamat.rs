//! Regression expectations independently checked against the cited notation.
use naghma_core::maqam::analysis::{AnalyzerConfig, Evidence, HistoryNote, MaqamAnalyzer};
use naghma_core::maqam::{JinsRole, MaqamDataset, SCHEMA_VERSION};
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

#[test]
fn sikah_has_a_separate_huzam_route_with_hijaz_on_the_third() {
    let ds = MaqamDataset::teaching_eight_24edo();
    let sikah = ds.maqam("sikah").unwrap();
    assert_eq!(
        names("sikah", "Mi half-flat 4", Some("huzam-route")),
        [
            "Mi half-flat 4",
            "Fa4",
            "Sol4",
            "La flat 4",
            "Si4",
            "Do5",
            "Re5",
            "Mi half-flat 5"
        ]
    );
    let placements = sikah.placements(Some("huzam-route")).unwrap();
    assert!(
        placements
            .iter()
            .any(|p| p.jins == "hijaz" && p.on_step == 2)
    );
    // The primary illustration is unchanged.
    assert_eq!(
        names("sikah", "Mi half-flat 4", None)[3..5],
        ["La4", "Si half-flat 4"]
    );
}

/// A small taqsim sketch: (pitch, seconds) passages played back to back and
/// analysed every second, as a live application does. Returns the analysis
/// after each passage.
fn perform(passages: &[&[(f64, f64)]]) -> Vec<naghma_core::maqam::analysis::MaqamAnalysis> {
    let ds = MaqamDataset::teaching_eight_24edo();
    let mut a = MaqamAnalyzer::new(AnalyzerConfig::default());
    let (mut t, mut next_analysis) = (0.0, 1.0);
    let mut out = Vec::new();
    for passage in passages {
        let mut last = None;
        for &(pitch, duration) in *passage {
            a.push(HistoryNote {
                start_s: t,
                duration_s: duration,
                pitch,
            });
            t += duration;
            while next_analysis <= t {
                last = Some(a.analyze(Some(&ds), None, next_analysis));
                next_analysis += 1.0;
            }
        }
        out.push(last.expect("a passage longer than a second"));
    }
    out
}

/// Nahawand on Sol4, rising to Re5 and coming to rest on Sol (the ghammaz
/// of Hijaz and Bayati on Re).
const UPPER_NAHAWAND_ON_SOL: [(f64, f64); 9] = [
    (67.0, 0.4),
    (69.0, 0.4),
    (70.0, 0.4),
    (72.0, 0.6),
    (74.0, 0.8),
    (72.0, 0.4),
    (70.0, 0.4),
    (69.0, 0.4),
    (67.0, 1.2),
];

fn upper_passage_keeps_the_maqam(id: &str, lower: &[(f64, f64)], lower_jins: &str) {
    let opening: Vec<(f64, f64)> = lower.iter().chain(lower).copied().collect();
    let upper: Vec<(f64, f64)> = UPPER_NAHAWAND_ON_SOL
        .iter()
        .cycle()
        .take(3 * UPPER_NAHAWAND_ON_SOL.len())
        .copied()
        .collect();
    let readings = perform(&[&opening, &upper, lower]);
    let re = NoteSpelling::new(note("Re4").degree, note("Re4").accidental);
    let sol = NoteSpelling::new(note("Sol4").degree, note("Sol4").accidental);
    for (i, r) in readings.iter().enumerate() {
        assert_eq!(r.candidates[0].maqam, id, "passage {i}: {r:#?}");
        assert_eq!(r.candidates[0].tonic, re, "passage {i}: {r:#?}");
        assert!(!r.possible_modulation, "passage {i}: {r:#?}");
    }
    // Fifteen seconds in the upper jins: its second jins, not Nahawand.
    let upper = readings[1].active_jins.as_ref().expect("an active jins");
    assert_eq!(upper.jins, "nahawand");
    assert_eq!(upper.role, JinsRole::Upper);
    assert_eq!((upper.on_step, upper.root), (3, sol));
    // Back down to the qarar: the first jins again.
    let back = readings[2].active_jins.as_ref().expect("an active jins");
    assert_eq!(
        (back.jins.as_str(), &back.role),
        (lower_jins, &JinsRole::Lower)
    );
    assert_eq!(back.root, re);
}

#[test]
fn hijaz_upper_nahawand_is_its_second_jins_not_maqam_nahawand() {
    // Hijaz on Re4: Re Mi-flat Fa-sharp Sol, resting on Re.
    let lower = [
        (62.0, 0.4),
        (63.0, 0.4),
        (66.0, 0.5),
        (67.0, 0.8),
        (66.0, 0.4),
        (63.0, 0.4),
        (62.0, 1.5),
    ];
    upper_passage_keeps_the_maqam("hijaz", &lower, "hijaz");
}

#[test]
fn bayati_upper_nahawand_is_its_second_jins_not_maqam_nahawand() {
    // Bayati on Re4: Re Mi-half-flat Fa Sol, resting on Re.
    let lower = [
        (62.0, 0.4),
        (63.5, 0.4),
        (65.0, 0.5),
        (67.0, 0.8),
        (65.0, 0.4),
        (63.5, 0.4),
        (62.0, 1.5),
    ];
    upper_passage_keeps_the_maqam("bayati", &lower, "bayati");
}

#[test]
fn leaving_bayati_for_saba_is_a_possible_modulation() {
    // Bayati on Re4, then Saba on Re: Re Mi-half-flat Fa Sol-flat.
    let bayati = [
        (62.0, 0.4),
        (63.5, 0.4),
        (65.0, 0.5),
        (67.0, 0.8),
        (65.0, 0.4),
        (63.5, 0.4),
        (62.0, 1.5),
    ];
    let saba = [
        (62.0, 0.4),
        (63.5, 0.4),
        (65.0, 0.4),
        (66.0, 0.8),
        (65.0, 0.4),
        (63.5, 0.4),
        (62.0, 1.0),
    ];
    let opening: Vec<(f64, f64)> = bayati.iter().chain(&bayati).copied().collect();
    let readings = perform(&[&opening, &saba]);
    assert_eq!(readings[0].candidates[0].maqam, "bayati");
    let r = &readings[1];
    let moved = r.candidates[0].maqam == "saba";
    let flagged = r
        .foreign_jins
        .as_ref()
        .is_some_and(|j| j.jins == "saba" && r.possible_modulation);
    assert!(moved || flagged, "{r:#?}");
}

#[test]
fn one_hijaz_phrase_through_both_ajnas_is_not_nahawand_on_its_fourth() {
    // Without any history: Hijaz on Re4 up through its upper Nahawand to Re5
    // and back to rest on Re4. Nahawand on Sol has exactly the same notes;
    // only where the phrase rests (cadence) and which jins lies at the
    // bottom (register) tell them apart.
    let phrase = [
        (62.0, 0.4),
        (63.0, 0.4),
        (66.0, 0.4),
        (67.0, 0.6),
        (69.0, 0.4),
        (70.0, 0.4),
        (72.0, 0.4),
        (74.0, 0.6),
        (72.0, 0.4),
        (70.0, 0.4),
        (69.0, 0.4),
        (67.0, 0.8),
        (66.0, 0.4),
        (63.0, 0.4),
        (62.0, 1.2),
    ];
    let ds = MaqamDataset::teaching_eight_24edo();
    let config = AnalyzerConfig::default();
    let mut a = MaqamAnalyzer::new(config);
    let mut t = 0.0;
    for &(pitch, duration_s) in &phrase {
        a.push(HistoryNote {
            start_s: t,
            duration_s,
            pitch,
        });
        t += duration_s;
    }
    let r = a.analyze(Some(&ds), None, t + 0.5);
    let score = |id: &str| r.candidates.iter().find(|c| c.maqam == id).unwrap().score;
    assert_eq!(r.candidates[0].maqam, "hijaz", "{r:#?}");
    assert!(
        score("hijaz") - score("nahawand") > config.ambiguity_margin,
        "{r:#?}"
    );
}

#[test]
fn a_real_modulation_still_takes_over() {
    // Thirty seconds of Bayati on Re, then Hijaz on Re: the same tonic, but
    // Mi-flat and Fa-sharp contradict Bayati. The history that holds a maqam
    // through its upper jins must not hold it through a modulation.
    let bayati = [
        (62.0, 0.4),
        (63.5, 0.4),
        (65.0, 0.5),
        (67.0, 0.8),
        (69.0, 0.4),
        (67.0, 0.4),
        (65.0, 0.4),
        (63.5, 0.4),
        (62.0, 1.5),
    ];
    let hijaz = [
        (62.0, 0.4),
        (63.0, 0.4),
        (66.0, 0.5),
        (67.0, 0.8),
        (69.0, 0.4),
        (67.0, 0.4),
        (66.0, 0.4),
        (63.0, 0.4),
        (62.0, 1.5),
    ];
    let opening: Vec<(f64, f64)> = bayati
        .iter()
        .cycle()
        .take(6 * bayati.len())
        .copied()
        .collect();
    // Four phrases, 20 s.
    let after: Vec<(f64, f64)> = hijaz
        .iter()
        .cycle()
        .take(4 * hijaz.len())
        .copied()
        .collect();
    let readings = perform(&[&opening, &after]);
    assert_eq!(readings[0].candidates[0].maqam, "bayati");
    assert_eq!(
        readings[1].candidates[0].maqam, "hijaz",
        "{:#?}",
        readings[1]
    );
}
