# naghma-core

Arabic music theory and microtonality for software, plus a reusable
real-time pitch-analysis pipeline.

| Package | Purpose | Depends on |
|-------|---------|------------|
| [`naghma-core`](crates/naghma-core) | Fixed-Do solfège (Latin & Arabic), quarter-tones, tuning systems, transposition (sounding vs. player pitch), oud instrument models, fingerboard geometry, note tracking, maqam data structures | `serde`, `serde_json` |
| [`naghma-audio`](crates/naghma-audio) | CPAL microphone capture, lock-free buffering, DSP worker producing `naghma-core` observations; labelled simulation source | `naghma-core`, [`pitchforge`](https://github.com/MansourAZ0/pitchforge-rs), `cpal`, `rtrb` |
| [`@naghma/ui`](packages/naghma-ui) | Shared React rendering: solfège labels, SVG quarter-tone accidentals, live frame store, cents meter | `react` (peer) |

`naghma-core` is UI-framework independent and does no I/O. Applications
(*watar-tuner*, *Ajnas*) consume these crates instead of copying code from
each other.

> **Status:** 0.x, early development. See [docs/PROVISIONAL.md](docs/PROVISIONAL.md)
> for musical decisions awaiting confirmation.

## Core ideas

```text
frequency ──► Pitch (continuous, never rounded)
                 │ Transposition (m_player = m_sounding − t)
                 ├──────────────► player pitch ──► TuningSystem ──► SpelledNote + cents
                 └──────────────► sounding pitch ─► TuningSystem ──► SpelledNote + cents
```

* **Two identities per note.** Every reading has a *sounding* (concert)
  note and a *player* note. An oud tuned −2 semitones that sounds Do4 is
  shown to the player as **Re4**. The measured frequency is never altered.
* **Solfège only.** Do Re Mi Fa Sol La Si / دو ري مي فا صول لا سي.
  No Western letter names exist in the API.
* **Quarter-tones are first-class.** Accidentals are integer quarter-tone
  steps (−4…+4); 24-EDO is the default tuning, custom intonation profiles
  can place targets anywhere.
* **Octaves follow spelling.** *Do half-flat 4* sounds below Do4 but is
  written in octave 4.
* **No invented maqam data.** The maqam module is a schema plus a
  validating loader. Datasets must declare provenance.

## Example

```rust
use naghma_core::interpret::PitchContext;
use naghma_core::notation::{LabelOptions, Script};
use naghma_core::pitch::{IntervalUnit, Transposition};

let ctx = PitchContext {
    transposition: Transposition::new(-2.0, IntervalUnit::Semitone),
    ..PitchContext::default()
};
let reading = ctx.read(261.63).unwrap();
assert_eq!(reading.sounding.note.to_string(), "Do4");
assert_eq!(reading.player.note.to_string(), "Re4");

let arabic = reading.player.note.label(&LabelOptions { script: Script::Arabic, ..Default::default() });
assert_eq!(arabic.text, "ري4");
```

Live analysis:

```sh
cargo run --release -p naghma-audio --example listen -- 10              # microphone
cargo run --release -p naghma-audio --example listen -- 10 --simulate   # synthetic, labelled
```

## Using from another repository

```toml
[dependencies]
naghma-core  = { git = "https://github.com/MansourAZ0/naghma-core", tag = "v0.1.0" }
naghma-audio = { git = "https://github.com/MansourAZ0/naghma-core", tag = "v0.1.0" }
```

For local development against sibling checkouts, create a gitignored
`.cargo/config.toml` in the consuming repository:

```toml
[patch."https://github.com/MansourAZ0/naghma-core"]
naghma-core  = { path = "../naghma-core/crates/naghma-core" }
naghma-audio = { path = "../naghma-core/crates/naghma-audio" }

[patch."https://github.com/MansourAZ0/pitchforge-rs"]
pitchforge = { path = "../pitchforge-rs" }
```

While the repositories are private, Cargo needs `net.git-fetch-with-cli =
true` (so it uses your git credentials) and CI needs a `DEPS_TOKEN` secret
with read access to the dependency repositories.

## Platform notes

* **macOS:** microphone access requires permission. Opening the device
  blocks while the permission prompt is shown; `Engine::start` gives up
  after 20 s with `EngineError::StartTimeout`. Command-line examples are
  attributed to the terminal app that launched them.
* **Linux:** needs ALSA development headers (`libasound2-dev`) for CPAL.
* **Windows:** untested so far.

## Development

```sh
cargo test --workspace --release
cargo clippy --workspace --all-targets
```

MSRV: Rust 1.85.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
