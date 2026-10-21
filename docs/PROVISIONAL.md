# Provisional musical decisions

These choices are implemented so the software works, but they are **not
authoritative** and need confirmation from a musician before they are
treated as correct. Each is data or configuration, so changing it does not
require changing algorithms.

| Item | Current choice | Where |
|------|----------------|-------|
| Arabic accidental words | بيمول (flat), نصف بيمول (half-flat), دييز (sharp), نصف دييز (half-sharp), دوبل بيمول / دوبل دييز, بيمول ونصف / دييز ونصف | `notation.rs` `Accidental::arabic_name` |
| English names for ±3 quarter-tones | "flat-and-a-half", "sharp-and-a-half" | `Accidental::english_name` |
| Quarter-tone glyphs | U+1D133 𝄳 (half-flat), U+1D132 𝄲 (half-sharp) | `Accidental::symbol` |
| Default 24-EDO spelling | Do, Do𝄲, Do♯, Re𝄳, Re, Re𝄲, Mi♭, Mi𝄳, Mi, Mi𝄲, Fa, Fa𝄲, Fa♯, Sol𝄳, Sol, Sol𝄲, La♭, La𝄳, La, La𝄲, Si♭, Si𝄳, Si, Si𝄲 | `tuning.rs` `SpellingTable::default_24` |
| Octave numbering | Scientific: Do4 ≈ 261.63 Hz at La4 = 440 Hz | `notation.rs` |
| Example oud profile | Do2 Fa2 La2 Re3 Sol3 Do4, lowest course single; 600 mm scale, 300 mm fingerboard | `data/instruments/oud-6-course-example.json` |
| Transposition unit | "−1/−2" read as semitones by default; unit is stored explicitly and can be whole tones | `pitch.rs` `Transposition` |

## Missing data (to be supplied)

* Maqam definitions (scales, ajnas, resting tones, phrases, modulations).
* Jins definitions.
* Intonation profiles for maqam performance (non-24-EDO targets).
* Context-dependent enharmonic spelling rules per maqam.

Until supplied, the only maqam dataset is
`crates/naghma-core/tests/fixtures/synthetic-maqam-dataset.json`, marked
`syntheticTestData` with placeholder names.
