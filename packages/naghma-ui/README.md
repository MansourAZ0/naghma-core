# @naghma/ui

Shared React pieces for apps built on `naghma-core` (watar-tuner, Ajnas):

- `NoteName` — renders a `NoteLabel` from Rust with the chosen note script and accidental style,
- `AccidentalGlyph` — quarter-tone accidentals drawn as SVG (no music font needed),
- `LiveStore` / `useLive` — high-rate frame store with selective React updates,
- `CentsMeter` — canvas tuning meter with a critically damped needle,
- tuning helpers (zones, spring, pitch-class hue, formatting).

Musical interpretation (naming, transposition) stays in Rust; this package
only renders.

Consumed as a local path dependency during development
(`"@naghma/ui": "file:../naghma-core/packages/naghma-ui"`); apps must dedupe
React (`resolve.dedupe: ["react", "react-dom"]` in Vite).


Install this package's dependencies before building a sibling app:

```sh
npm ci --prefix ../naghma-core/packages/naghma-ui
```

Run that command from the app directory, then run the app's own `npm ci`.
TypeScript resolves the linked source's React types from this package's tree.
Use the same naghma release tag as the consuming app documents. See the root
[credits](../../CREDITS.md), [contribution guide](../../CONTRIBUTING.md), and
[license notices](../../THIRD_PARTY_NOTICES.md).
