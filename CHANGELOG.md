# Changelog

All notable changes are documented here ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
[SemVer](https://semver.org/); 0.x minor versions may break the API).

## [Unreleased]

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
