# naghma-core · نغمة

The shared music engine behind Watar and Ajnas.

`naghma-core` turns a measured frequency into a useful musical reading: a note
name, octave, and distance from the target pitch. It supports Arabic and Latin
solfège, quarter-tones, oud tunings, fingerboard positions, and sourced maqam
illustrations.

**Early development:** the libraries are 0.x and their APIs can change.
The current focus is Arabic maqam music and the oud, rather than every Eastern
music tradition.

## The three packages

| Package | What it does | Needs a microphone? |
| --- | --- | --- |
| [`naghma-core`](crates/naghma-core) | Music theory, naming, tuning, note tracking, maqam data and ranking. No device or file I/O. | No |
| [`naghma-audio`](crates/naghma-audio) | Microphone capture and a background analysis worker. Also supports simulation and offline audio. | Optional |
| [`@naghma/ui`](packages/naghma-ui) | React note labels, quarter-tone symbols, a tuning meter and live display helpers. | No |

The audio worker uses [pitchforge-rs](https://github.com/MansourAZ0/pitchforge-rs)
for pitch detection. [Watar](https://github.com/MansourAZ0/watar-tuner) and
[Ajnas](https://github.com/MansourAZ0/Ajnas) use these packages independently;
neither application depends on the other.

```text
audio → pitchforge → frequency → naghma-core → note names and cents
                         ↑
               naghma-audio handles capture and streaming
```

## What makes it useful for Arabic music?

- **Quarter-tones:** the default grid divides an octave into 24 equal steps.
  Each step is 50 cents; 100 cents is one semitone.
- **Flexible intonation:** custom profiles can place targets between grid steps.
  Real maqam intonation is not universally equal-tempered.
- **Fixed-Do solfège:** Do Re Mi Fa Sol La Si, or دو ري مي فا صول لا سي,
  with quarter-tone accidentals.
- **Player and sounding names:** if an oud is tuned two semitones low, the
  player's Re4 sounds Do4. Both names are kept, along with the original Hz.
- **Oud geometry:** calculate possible finger positions from string length and
  tuning. The bundled six-course tuning is an example, not a universal standard.
- **Maqam structure:** source-labelled ajnas, upper-jins alternatives,
  transposition, and register-specific Saba examples.
- **Imperfect playing:** recognition uses a configurable tolerance and can
  estimate an instrument's overall tuning offset.

These are additions around a general-purpose pitch detector. They do not require
changing a frequency into a Western note first. See [credits](CREDITS.md) for
algorithm authorship, related prior work, and the boundary between the libraries.

## Try it

Install current stable Rust. Linux microphone builds also need ALSA headers
(`libasound2-dev` on Debian/Ubuntu).

```sh
git clone https://github.com/MansourAZ0/naghma-core.git
cd naghma-core
cargo test --workspace --locked
cargo run --release -p naghma-audio --example listen -- 10 --simulate
```

The simulation plays generated notes through the analysis pipeline and is
labelled as synthetic. To listen through your microphone instead:

```sh
cargo run --release -p naghma-audio --example listen -- 10
```

On macOS, allow microphone access for the launching terminal or app. Opening the
device times out after 20 seconds if the permission prompt is left unanswered.
Windows device behaviour still needs field validation.

## Use the library

The current Git release is `v0.8.0` (it requires pitchforge `v0.1.1`).
Add only the crates you need:

```toml
[dependencies]
naghma-core = { git = "https://github.com/MansourAZ0/naghma-core", tag = "v0.8.0" }
naghma-audio = { git = "https://github.com/MansourAZ0/naghma-core", tag = "v0.8.0" }
```

```rust
use naghma_core::interpret::PitchContext;
use naghma_core::pitch::{IntervalUnit, Transposition};

let context = PitchContext {
    transposition: Transposition::new(-2.0, IntervalUnit::Semitone),
    ..PitchContext::default()
};
let reading = context.read(261.63).unwrap();
assert_eq!(reading.sounding.note.to_string(), "Do4");
assert_eq!(reading.player.note.to_string(), "Re4");
```

For audio without device capture, set `default-features = false` on
`naghma-audio`. The pure `naghma-core` crate has no audio-device dependency.

## Maqam data and recognition

`MaqamDataset::teaching_eight_24edo()` loads optional illustrations of Saba,
Nahawand, Ajam, Bayati, Sikah, Hijaz, Rast and Kurd. Their intervals approximate
written notation; they are **not exact rules for performance intonation**.
See the [source record](docs/MAQAMAT_RESEARCH.md),
[provisional choices](docs/PROVISIONAL.md), and
[interactive tonic reference](docs/reference/maqamat-tonics.html).

Recognition ranks possible maqamat from recent notes. Scores are heuristics,
not probabilities, and a pitch set alone does not describe a complete maqam.
It weighs where phrases come to rest (qaflat) and keeps a fading memory of
earlier material, so a passage in a maqam's upper jins stays that maqam. Each
analysis also names the jins the latest passage dwells in (first jins or an
upper one) and a jins outside the maqam when a modulation may be under way.
The recorded evaluation reports 73% top-one and 91% top-three accuracy on its
held-out live analyses; Sikah remains weak. These results come from a small
corpus whose labels ignore modulation; they are not a guarantee. See
[evaluation methods, recordings and limitations](docs/EVALUATION.md).

## Development

```sh
cargo fmt --all --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cd packages/naghma-ui
npm ci
npm run typecheck
npm test
```

The pure crate declares Rust 1.85 as its minimum. The audio package and full
workspace should be developed with current stable Rust. The UI uses Node 24+.
See [contributing](CONTRIBUTING.md) for local dependency overrides.

## Credits and license

The pitch layer implements YIN by **Alain de Cheveigné and Hideki Kawahara**.
[Alessandro Genova's pitch-detection](https://github.com/alesgenova/pitch-detection)
is acknowledged as related Rust prior work; a source-code fork is not established
by the available history. Arabic music interpretation and oud support live here.
See [credits](CREDITS.md) and [third-party notices](THIRD_PARTY_NOTICES.md).

Project code: [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Read the [release guide](docs/OPEN_SOURCE.md) before publishing source or binaries.
