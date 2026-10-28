# Changelog

All notable changes are documented here ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
[SemVer](https://semver.org/); 0.x minor versions may break the API).

## [Unreleased]

### Fixed
- `EngineSession`: a `stop` while a slow `start` was opening the device left
  the engine running afterwards (reproduced), and overlapping starts could
  leave two engines. Every start and stop now takes a generation number;
  the last call wins and a replaced start returns the new
  `EngineError::Superseded` (serialized `superseded`). A `reconfigure` during
  a start returns the new `Reconfigured::Pending` and is applied when the
  device opens.
- Note tracker: a pluck's first ~100 ms no longer becomes a separate note.
  For `settleS` (0.12 s) after an attack the note is renamed in place and
  reported as the new `TrackerUpdate::revised` (same id as `started`).
- Note tracker: readings an octave, a twelfth or two octaves from a sounding
  note are skipped while the level decays (`harmonicGuard`): a harmonic, not
  a new note. On 83 minutes of recorded oud (new `tracker_artifacts`
  example) split plucks fell from 9.1 to 0.8 per minute and octave flips
  from 26.7 to 5.4 per minute. Maqam recognition did not get worse.
- Validate EqualTemperament divisions during JSON deserialization.
- Report non-finite audio windows without panicking; reset onset history and recover on valid input.

### Changed
- The maqam window is now **10 s** (was 30 s), so a taqsim's sections in
  different maqamat are no longer mixed together. The instrument's tuning
  offset is still estimated over the last 60 s (`tuningWindowS`), and a small
  `continuity` weight (0.1) keeps the current reading through near-ties.
  Development / held-out share of analyses naming the clip's stated maqam:
  71% / 63% at 30 s -> 54% / 46% at 10 s (a plain 10 s window gives 50% / 44%).
  Scale demos and doulabs, which stay in one maqam, are unchanged (74% vs 75%);
  the difference is in taqasim, whose clip labels ignore modulation.
- Add ObservationStatus::InvalidInput (serialized as `invalidInput`). Downstream
  exhaustive matches must handle this new variant before adopting the next release.
- Public CI no longer requires cross-repository dependency tokens; UI installs use npm ci.

### Added
- `AnalysisConfig::fast_attack` (default on): until a plucked note is shown,
  read it from the newest 43 ms instead of the full 85 ms window. Median
  attack-to-display time on recorded oud 85 ms -> 64 ms, with no more
  tracking artefacts. `NoteTracker::awaiting_attack` reports that moment.
- Sikah alternative "Huzam route" (Sikah; Hijaz on 3; Rast on 6, sourced
  from MaqamWorld). Sikah recognition at 10 s: 28% -> 43% (development) and
  23% -> 50% (held-out) of analyses.
- `measure_intonation` and `tracker_artifacts` examples for real recordings.
  Measured performed intonation is documented in docs/EVALUATION.md; it did
  not improve recognition, so maqam data keeps its 24-EDO values.
- Accessible README, credits, privacy/contribution/release guides, audits, source
  exporter, secret/dependency checks and dependency update configuration.

## [0.5.0] - 2026-10-25

Measured against 85 labelled public oud recordings (docs/EVALUATION.md):
share of live analyses naming the stated maqam 49% -> 71% (development set)
and 44% -> 63% (held-out set); top-3 82% -> 90% / 77% -> 88%.

### naghma-core
- Soft scale-degree membership: `tolerance_cents` is now an accepted
  intonation error (full credit within half of it, half at it, none at 1.5x)
  instead of a hard cutoff. A degree or jins note counts as heard only after
  0.12 s of matching time.
- Global tuning-offset compensation: `estimate_tuning_offset()` (bounded to
  +/-20 c, ignores quarter-tones); `AnalyzerConfig::tuning_offset_cents`
  (`None` = estimate, `Some(x)` = fixed) and `max_tuning_offset_cents`;
  `MaqamAnalysis::tuning_offset_cents` reports the applied offset.
- Nonperiodic maqamat (Saba): notes outside the documented register are
  compared an octave at a time with the nearest documented register; anchors
  need at least half the played time inside their register.
- `AnalyzerConfig::min_note_s` (0.15 s) ignores pluck transients.
- `ScoreWeights::final_rest` separates the "latest long note on the tonic"
  term (default 0; it flipped results phrase by phrase); tonic-time weight 0.4.
- Default window 30 s (was 25 s).
- `membership()` is public.
- `NoteTracker`: a note never starts before the previous one ended
  (attack-started notes could overlap).
- Serialized output changes are additive (`tuningOffsetCents`); `ScoreWeights`
  and `AnalyzerConfig` gain fields (0.x minor version bump).

### naghma-audio
- `eval_maqam` example: per-clip and aggregate maqam/tonic accuracy over a
  manifest of labelled recordings, with JSON config overrides for A/B tests.

## [0.4.0] - 2026-10-22

- Add a sourced, provisional dataset for the eight maqamat in صنع بسحرك,
  with 24-EDO notation approximations and per-entry citations.
- Schema 2: explicit alternative formulas, octave-periodicity and upper-jins
  tonic placement. Legacy schema 1 remains readable.
- Add variant realization and range-target APIs. Nonperiodic Saba illustrations
  stay within their documented register.
- Score alternatives independently and report the selected variant; use absolute
  register-aware comparisons for nonperiodic candidates.
- Public Maqam/Jins/Candidate structs gain fields: explicit 0.x API version bump.
- Add reference-spelling, all-tonic, Bayati Re/Sol/La and recognition regressions.


## [0.3.1] - 2026-10-22

- Reject corrected fingerboard positions behind the nut and non-finite positions.
- Validate calibration so the corrected endpoint remains within the vibrating string.
- Add regressions for negative offsets and invalid corrected geometry.

## [0.3.0] - 2026-10-21

### naghma-core
- `preferences::NotationPreferences` (+ `TuningChoice`): shared notation
  settings with conversions to `PitchContext` / `LabelOptions`.
- `InstrumentProfile::fingerboard_map`: all playable named notes with positions.

## [0.2.0] - 2026-10-21

### naghma-core
- `maqam::analysis`: rule-based maqam candidate scoring over a rolling
  note history (fit, foreign notes, coverage, tonic, resting tones, ajnas,
  phrases), with InsufficientData / Unknown / Ambiguous / Likely status and
  a possible-modulation flag. Heuristic, data-driven, no built-in maqamat.

### naghma-audio
- `EngineSession`: shared engine owner for app backends (live reconfigure,
  restart on source/analysis change).
- `analyze_wav` example for validating real recordings.

### @naghma/ui (new)
- Shared React rendering: `NoteName`, `AccidentalGlyph`, `LiveStore`/`useLive`,
  `CentsMeter`, tuning helpers.

## [0.1.0] - 2026-10-21

### naghma-core
- Continuous `Pitch`, `ReferencePitch`, `Interval`, `Transposition` with explicit units.
- Fixed-Do solfège notation (Latin/Arabic), quarter-tone `Accidental`, `SpelledNote`, labels.
- `EqualTemperament` (24-EDO default) with configurable `SpellingTable`; `IntonationProfile`.
- `PitchContext` / `NoteReading` with sounding and player identities.
- `InstrumentProfile` with fretless fingerboard geometry; example 6-course oud.
- `PitchObservation`, `MusicalNoteEvent`; `NoteTracker` (gating, hysteresis, onsets, smoothing).
- Maqam/jins schema and validating loader (no musical data).

### naghma-audio
- `Analyzer` (Framer + YIN + onsets + tracker), offline `analyze_samples`.
- `Engine` with CPAL capture, SPSC ring buffer, DSP thread, simulation source, start timeout.
- `listen` example.
