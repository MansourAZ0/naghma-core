# @naghma/ui

Shared React pieces for apps built on `naghma-core` (watar-tuner, Ajnas):

- `NoteName` — renders a `NoteLabel` from Rust, with
- `AccidentalGlyph` — quarter-tone accidentals drawn as SVG (no music font needed),
- `LiveStore` / `useLive` — high-rate frame store with selective React updates,
- `CentsMeter` — canvas tuning meter with a critically damped needle,
- tuning helpers (zones, spring, pitch-class hue, formatting).

Musical interpretation (naming, transposition) stays in Rust; this package
only renders.

Consumed as a local path dependency during development
(`"@naghma/ui": "file:../naghma-core/packages/naghma-ui"`); apps must dedupe
React (`resolve.dedupe: ["react", "react-dom"]` in Vite).
