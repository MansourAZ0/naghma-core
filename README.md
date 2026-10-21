# naghma-core

Arabic music theory and microtonality for software, plus a reusable
real-time pitch-analysis pipeline.

| Crate | Purpose | Depends on |
|-------|---------|------------|
| [`naghma-core`](crates/naghma-core) | Fixed-Do solfège (Latin & Arabic), quarter-tones, tuning systems, transposition (sounding vs. player pitch), oud instrument models, fingerboard geometry, note tracking, maqam data structures | `serde` only |
| [`naghma-audio`](crates/naghma-audio) | CPAL microphone capture, lock-free buffering, DSP worker producing `naghma-core` observations | `naghma-core`, [`pitchforge`](https://github.com/MansourAZ0/pitchforge-rs), `cpal` |

`naghma-core` is UI-framework independent and does no I/O. Applications
(e.g. *watar-tuner*, *Ajnas*) consume these crates; they do not copy code
from each other.

> Status: 0.x, early development.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
