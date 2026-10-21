# Changelog

All notable changes are documented here ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
[SemVer](https://semver.org/); 0.x minor versions may break the API).

## [Unreleased]

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
