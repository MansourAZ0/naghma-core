# Repository audit — 2026-10-27

Baseline commit: `148b615b61fe` plus the local preparation changes.

**Architecture: 8.5/10. Overall engineering maturity: 7.5/10.** Scores describe
the audited working source. This is the central reusable asset for Arabic music.

## Architecture

The workspace correctly separates the pure theory crate from CPAL/device I/O.
The callback downmixes into an rtrb ring buffer; a dedicated worker frames audio,
runs pitch analysis and tracks notes. Apps get derived frames rather than raw
audio. The shared React package renders labels and meters while Rust owns music
interpretation. Continuous pitch and player/sounding identities are good domain
boundaries. Sourced, provisional maqam variants are kept as data.

Evidence: [theory modules](../crates/naghma-core/src/lib.rs),
[capture](../crates/naghma-audio/src/capture.rs),
[worker](../crates/naghma-audio/src/engine.rs),
[analysis](../crates/naghma-audio/src/analysis.rs),
[session ownership](../crates/naghma-audio/src/session.rs),
[shared UI](../packages/naghma-ui/src/index.ts).

## Findings and changes

| Priority | Finding | Result |
| --- | --- | --- |
| High | EqualTemperament JSON deserialization bypassed the constructor's division validation, permitting invalid internal state. | Fixed using validated deserialization; valid round-trip and invalid-division tests added. |
| High | Analyzer assumed detector failures only meant the wrong window length. NaN/infinite audio could panic its worker and poison onset history. | Fixed: emit InvalidInput with no pitch and zero confidence; reset onset state, recover on valid windows; test NaN and both infinities. |
| Medium | Public CI depended on cross-repo private tokens; shared UI installs were not locked. | Removed private authentication setup, used npm ci, and added audit checks. |
| Medium | README mixed theory explanation, private setup and evaluation claims. | Rewritten with simple definitions, complete setup and clearly historical evaluation limits. |
| Privacy | Git history includes personal email metadata. | Added history-free source export and publication instructions. |

`ObservationStatus::InvalidInput` is an added enum variant: downstream exhaustive
Rust matches must handle it, and TypeScript mirrors should accept `invalidInput`.
Release notes record this change. Apps still consume older tagged libraries.

## Remaining work

- Release new pitchforge and naghma tags and update consumers in dependency order.
- `EngineSession` releases its state lock during source startup. Concurrent
  start/stop/reconfigure operations need explicit lifecycle serialization or
  cancellation/generation tests. Ajnas adds an outer runtime lock; direct library
  users and Watar need care. This is a code-review concern, not a reproduced race
  in this pass.
- Broaden validation for directly constructed/deserialized public configurations,
  oversized simulation requests, extreme sample rates and tuning ranges.
- Document dropped-audio discontinuity policy: counters record drops, but the
  analyser's sample clock counts accepted samples. Long overloads need dedicated
  continuity and timestamp tests.
- Generate TypeScript boundary types or verify all IPC schemas automatically;
  some types are maintained by hand.
- Review musical data with musicians and extend the evaluation corpus. Sikah,
  phrase context and shared pitch sets remain limitations; 24-EDO is approximate.

## Validation

On macOS, Rust 1.99.1 and Node 26.12.1: **90 Rust tests including doctests** and
**11 shared-UI tests** pass; UI typecheck, Rust formatting and Clippy with warnings
denied pass. RustSec reports no vulnerabilities or informational warnings in the
workspace lockfile; npm audit reports zero known vulnerabilities for the UI.
Gitleaks 8.30.1 reports no credential findings in scanned history or release source.

Historical recorded-corpus results were reviewed, not rerun. This pass did not
validate Rust 1.85, physical microphones, Windows/Linux execution or hosted CI.
See [release preparation](OPEN_SOURCE.md).
