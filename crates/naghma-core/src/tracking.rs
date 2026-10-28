//! Turning a stream of pitch observations into stable notes.
//!
//! [`NoteTracker`] consumes one [`PitchObservation`] per analysis hop (plus
//! an optional onset flag from a signal-level attack detector) and decides
//! what the musician should see:
//!
//! * **Confidence gating** — only voiced observations with sufficient
//!   confidence count as evidence. Nothing is shown without evidence.
//! * **Confirmation** — a note appears after `confirm_frames` consecutive
//!   observations agree on it.
//! * **Hysteresis** — once a note is shown, the pitch must move clearly
//!   closer to a neighbouring target (by `hysteresis_cents`) for
//!   `change_frames` observations before the name changes, so a pitch on a
//!   quarter-tone boundary does not flicker.
//! * **Onsets** — an attack ends the current note and starts a new one, even
//!   on the same pitch (tremolo produces one event per pluck). Observations
//!   whose analysis window mostly precedes the attack are ignored, since
//!   they mix the old and new notes.
//! * **Attack settling** — the first moments of a pluck often read a
//!   quarter-tone flat or an octave high. For `settle_s` after an attack the
//!   note is renamed in place (reported as [`TrackerUpdate::revised`])
//!   instead of ending it and starting another.
//! * **Harmonic guard** — while a note sounds without a new attack, readings
//!   an octave, a twelfth or two octaves away are a harmonic taken for the
//!   fundamental (a fretted note cannot jump that far without a pluck) and
//!   are skipped, unless the level rises back to the note's peak, as a pluck
//!   the attack detector missed would. (During settling such a reading
//!   renames the note instead: the attack itself may have been the error.)
//! * **Dropout bridging** — short runs of invalid observations do not end a
//!   note; `release_frames` consecutive ones do.
//! * **Smoothing** — the displayed pitch is the median of the last few
//!   observations followed by light exponential smoothing; it is reset at
//!   every note change so smoothing never drags across notes.

use crate::events::{MusicalNoteEvent, NoteStart, ObservationStatus, PitchObservation};
use crate::interpret::{NoteReading, PitchContext};
use crate::pitch::Pitch;
use crate::tuning::{NoteMatch, TuningSystem};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Parameters of [`NoteTracker`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackerConfig {
    /// Minimum detector confidence for an observation to count.
    pub min_confidence: f64,
    /// Consecutive agreeing observations needed to show a new note.
    pub confirm_frames: u32,
    /// Consecutive observations needed to switch to a different note
    /// without an attack.
    pub change_frames: u32,
    /// Consecutive invalid observations that end a note.
    pub release_frames: u32,
    /// Extra cents a pitch must be closer to a new target before the shown
    /// note changes.
    pub hysteresis_cents: f64,
    /// Number of observations in the median filter.
    pub median_frames: usize,
    /// Exponential smoothing factor for the displayed pitch, `(0, 1]`
    /// (1 = no smoothing).
    pub smoothing: f64,
    /// Observations whose analysis window lies more than this fraction
    /// before an onset are ignored as mixed.
    pub onset_mix_fraction: f64,
    /// Seconds after an attack during which the note may be renamed in
    /// place; 0 disables.
    #[serde(default = "default_settle")]
    pub settle_s: f64,
    /// Skip readings a harmonic away from the sounding note (see the
    /// module documentation).
    #[serde(default = "default_true")]
    pub harmonic_guard: bool,
}

fn default_settle() -> f64 {
    0.12
}

fn default_true() -> bool {
    true
}

/// How long after a pluck [`NoteTracker::awaiting_attack`] stays `true`.
pub const AWAIT_ATTACK_S: f64 = 0.25;

/// `true` if `cents` is within 60 cents of an octave, a twelfth or two
/// octaves (up or down).
fn harmonic_jump(cents: f64) -> bool {
    [1200.0, 1902.0, 2400.0]
        .iter()
        .any(|h| (cents.abs() - h).abs() < 60.0)
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.8,
            confirm_frames: 2,
            change_frames: 3,
            release_frames: 6,
            hysteresis_cents: 10.0,
            median_frames: 5,
            smoothing: 0.4,
            onset_mix_fraction: 0.5,
            settle_s: default_settle(),
            harmonic_guard: true,
        }
    }
}

/// What the live display should show after an update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LiveState {
    /// No signal.
    Silent,
    /// Signal present but no confirmed note (insufficient evidence).
    Listening,
    /// A confirmed note.
    Note(LiveNote),
}

/// A confirmed, currently sounding note with its smoothed reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveNote {
    /// Id of the [`MusicalNoteEvent`] this belongs to.
    pub event_id: u64,
    /// Smoothed reading. Its note matches are the *held* note (with
    /// hysteresis), and its deviations are relative to that note.
    pub reading: NoteReading,
    /// Confidence of the latest contributing observation.
    pub confidence: f64,
    /// Stream time the note started.
    pub start_s: f64,
    /// `true` if the latest observation was invalid and the note is being
    /// held through a short dropout.
    pub holding: bool,
}

/// Result of one [`NoteTracker::update`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerUpdate {
    /// Stream time of the observation.
    pub time_s: f64,
    /// Display state.
    pub state: LiveState,
    /// A note that ended with this update.
    pub ended: Option<MusicalNoteEvent>,
    /// A note that started with this update.
    pub started: Option<MusicalNoteEvent>,
    /// A note that started shortly before, renamed while its attack
    /// settled (same id; replaces what `started` reported).
    #[serde(default)]
    pub revised: Option<MusicalNoteEvent>,
    /// An attack was reported with this update.
    pub onset: bool,
}

#[derive(Debug, Clone)]
struct Active {
    event: MusicalNoteEvent,
    held: NoteMatch, // in the space used for decisions (player)
    smoothed: Pitch, // sounding pitch
    pitches: Vec<f64>,
    confidence_sum: f64,
    invalid_run: u32,
    change_candidate: Option<(NoteMatch, u32)>,
    last_confidence: f64,
    last_valid_s: f64,
    /// Attack time while the note may still be renamed in place.
    settling_until: Option<f64>,
}

#[derive(Debug, Clone)]
struct Pending {
    note: NoteMatch,
    count: u32,
    first_time_s: f64,
    start_kind: NoteStart,
    evidence: Evidence,
}

/// Statistics accumulated from valid observations.
#[derive(Debug, Clone, Default)]
struct Evidence {
    pitches: Vec<f64>,
    confidence_sum: f64,
    peak_level_db: f64,
}

impl Evidence {
    fn new(pitch: Pitch, obs: &PitchObservation) -> Self {
        Self {
            pitches: vec![pitch.0],
            confidence_sum: obs.confidence,
            peak_level_db: obs.level_db,
        }
    }

    fn add(&mut self, pitch: Pitch, obs: &PitchObservation) {
        self.pitches.push(pitch.0);
        self.confidence_sum += obs.confidence;
        self.peak_level_db = self.peak_level_db.max(obs.level_db);
    }
}

/// Streaming note tracker. See the [module documentation](self).
#[derive(Debug, Clone)]
pub struct NoteTracker {
    config: TrackerConfig,
    context: PitchContext,
    recent: VecDeque<f64>, // sounding pitches for the median filter
    active: Option<Active>,
    pending: Option<Pending>,
    last_onset_s: Option<f64>,
    /// End of the most recent note; a new note never starts before it.
    last_end_s: Option<f64>,
    next_id: u64,
}

impl NoteTracker {
    /// Create a tracker naming notes in `context`.
    pub fn new(config: TrackerConfig, context: PitchContext) -> Self {
        Self {
            recent: VecDeque::with_capacity(config.median_frames.max(1)),
            config,
            context,
            active: None,
            pending: None,
            last_onset_s: None,
            last_end_s: None,
            next_id: 1,
        }
    }

    /// Current configuration.
    pub fn config(&self) -> &TrackerConfig {
        &self.config
    }

    /// Current naming context.
    pub fn context(&self) -> &PitchContext {
        &self.context
    }

    /// Change the naming context (reference, transposition, tuning). The
    /// current note, if any, is ended so no event mixes two contexts.
    pub fn set_context(&mut self, context: PitchContext, now_s: f64) -> Option<MusicalNoteEvent> {
        self.context = context;
        self.pending = None;
        self.recent.clear();
        self.finish_active(now_s)
    }

    /// Process one observation. `onset` is `true` if a signal-level attack
    /// was detected at `onset_time_s` (which may precede `obs.time_s`).
    pub fn update(&mut self, obs: &PitchObservation, onset: Option<f64>) -> TrackerUpdate {
        let mut out = TrackerUpdate {
            time_s: obs.time_s,
            state: LiveState::Silent,
            ended: None,
            started: None,
            revised: None,
            onset: onset.is_some(),
        };

        if let Some(t_on) = onset {
            self.last_onset_s = Some(t_on);
            out.ended = self.finish_active(t_on);
            self.pending = None;
            self.recent.clear();
        }

        let mixed = self.last_onset_s.is_some_and(|t_on| {
            let before = (t_on - obs.window_start_s()).max(0.0);
            obs.window_s > 0.0 && before / obs.window_s > self.config.onset_mix_fraction
        });
        let valid_pitch = match (obs.status, obs.frequency_hz) {
            (ObservationStatus::Voiced, Some(f))
                if obs.confidence >= self.config.min_confidence && !mixed =>
            {
                Pitch::from_frequency(f, self.context.reference)
            }
            _ => None,
        };

        match valid_pitch {
            Some(sounding) => self.on_valid(obs, sounding, &mut out),
            None => self.on_invalid(&mut out),
        }

        out.state = match &self.active {
            Some(a) => LiveState::Note(self.live_note(a)),
            None if obs.status == ObservationStatus::Silence && self.pending.is_none() => {
                LiveState::Silent
            }
            None => LiveState::Listening,
        };
        out
    }

    /// `true` while no note has been shown since a pluck less than
    /// [`AWAIT_ATTACK_S`] before `now_s`: the moment a reading from a
    /// shorter window helps most.
    pub fn awaiting_attack(&self, now_s: f64) -> bool {
        self.active.is_none()
            && self
                .last_onset_s
                .is_some_and(|t| now_s - t <= AWAIT_ATTACK_S)
    }

    /// End the current note (e.g. when the stream stops).
    pub fn flush(&mut self, now_s: f64) -> Option<MusicalNoteEvent> {
        self.pending = None;
        self.recent.clear();
        self.finish_active(now_s)
    }

    fn median_pitch(&mut self, sounding: Pitch) -> Pitch {
        let n = self.config.median_frames.max(1);
        if self.recent.len() == n {
            self.recent.pop_front();
        }
        self.recent.push_back(sounding.0);
        let mut v: Vec<f64> = self.recent.iter().copied().collect();
        v.sort_by(f64::total_cmp);
        Pitch(v[v.len() / 2])
    }

    fn player_match(&self, sounding: Pitch) -> NoteMatch {
        let player = self.context.transposition.sounding_to_player(sounding);
        self.context.tuning.nearest(player)
    }

    fn on_valid(&mut self, obs: &PitchObservation, raw: Pitch, out: &mut TrackerUpdate) {
        if let Some(active) = &mut self.active {
            let settling = active.settling_until.is_some_and(|t| obs.time_s <= t);
            let rising = obs.level_db >= active.event.peak_level_db - 1.0;
            if self.config.harmonic_guard
                && !settling
                && !rising
                && harmonic_jump(active.smoothed.cents_to(raw))
            {
                // A harmonic, not a new note: hold the note as it is.
                active.invalid_run = 0;
                return;
            }
        }
        let filtered = self.median_pitch(raw);
        let candidate = self.player_match(filtered);

        if let Some(active) = &mut self.active {
            active.invalid_run = 0;
            if active.settling_until.is_some_and(|t| obs.time_s > t) {
                active.settling_until = None;
            }
            if active.settling_until.is_some() && candidate.note != active.held.note {
                // Still inside the attack: rename the note in place.
                active.held = candidate;
                active.smoothed = filtered;
                active.pitches.clear();
                active.pitches.push(filtered.0);
                active.change_candidate = None;
                active.last_confidence = obs.confidence;
                active.last_valid_s = obs.time_s;
                active.event.observations += 1;
                let reading = self.reading_for(filtered);
                let Some(active) = &mut self.active else {
                    return;
                };
                let event = &mut active.event;
                event.frequency_hz = reading.frequency_hz;
                event.sounding_pitch = reading.sounding_pitch;
                event.player_pitch = reading.player_pitch;
                event.player = candidate;
                event.sounding = reading.sounding;
                out.revised = Some(event.clone());
                return;
            }
            if candidate.note == active.held.note {
                active.change_candidate = None;
            } else {
                // Hysteresis: the new target must be clearly closer.
                let player = self.context.transposition.sounding_to_player(filtered);
                let to_held = active.held.target.cents_to(player).abs();
                let to_new = candidate.target.cents_to(player).abs();
                if to_new + self.config.hysteresis_cents < to_held {
                    let count = match active.change_candidate {
                        Some((m, c)) if m.note == candidate.note => c + 1,
                        _ => 1,
                    };
                    active.change_candidate = Some((candidate, count));
                    if count >= self.config.change_frames {
                        let t = obs.time_s;
                        out.ended = self.finish_active(t).or(out.ended.take());
                        self.recent.clear();
                        self.recent.push_back(filtered.0);
                        out.started = Some(self.start_note(
                            candidate,
                            filtered,
                            t,
                            NoteStart::PitchChange,
                            obs,
                            Evidence::new(filtered, obs),
                        ));
                        return;
                    }
                } else {
                    active.change_candidate = None;
                }
            }
            // Same note (or not yet switching): accumulate and smooth.
            let a = self.config.smoothing.clamp(0.01, 1.0);
            active.smoothed = Pitch(active.smoothed.0 + a * (filtered.0 - active.smoothed.0));
            active.pitches.push(filtered.0);
            active.confidence_sum += obs.confidence;
            active.last_confidence = obs.confidence;
            active.last_valid_s = obs.time_s;
            active.event.observations += 1;
            active.event.peak_level_db = active.event.peak_level_db.max(obs.level_db);
            return;
        }

        // No active note: build up confirmation. Consecutive observations
        // agree if they name the same note, or if the pending note is within
        // the hysteresis margin — otherwise a pitch sitting on a boundary
        // between two targets would alternate and never be confirmed.
        let player = self.context.transposition.sounding_to_player(filtered);
        let pending = match self.pending.take() {
            Some(mut p)
                if p.note.note == candidate.note
                    || p.note.target.cents_to(player).abs()
                        <= candidate.target.cents_to(player).abs()
                            + self.config.hysteresis_cents =>
            {
                p.count += 1;
                p.evidence.add(filtered, obs);
                p
            }
            _ => Pending {
                evidence: Evidence::new(filtered, obs),
                note: candidate,
                count: 1,
                first_time_s: obs.time_s,
                start_kind: if self.last_onset_s.is_some_and(|t| obs.time_s - t < 0.5) {
                    NoteStart::Attack
                } else {
                    NoteStart::PitchChange
                },
            },
        };
        if pending.count >= self.config.confirm_frames {
            let start = match pending.start_kind {
                NoteStart::Attack => self.last_onset_s.unwrap_or(pending.first_time_s),
                NoteStart::PitchChange => pending.first_time_s,
            }
            // An attack already claimed by the previous note (which then
            // changed pitch) must not make the two events overlap.
            .max(self.last_end_s.unwrap_or(f64::NEG_INFINITY));
            out.started = Some(self.start_note(
                pending.note,
                filtered,
                start,
                pending.start_kind,
                obs,
                pending.evidence,
            ));
        } else {
            self.pending = Some(pending);
        }
    }

    fn on_invalid(&mut self, out: &mut TrackerUpdate) {
        if let Some(active) = &mut self.active {
            active.invalid_run += 1;
            if active.invalid_run >= self.config.release_frames {
                // The note ended at its last valid observation.
                let end = active.last_valid_s;
                out.ended = self.finish_active(end).or(out.ended.take());
                self.recent.clear();
            }
        } else {
            self.pending = None;
        }
    }

    fn start_note(
        &mut self,
        held: NoteMatch,
        sounding: Pitch,
        start_s: f64,
        start_kind: NoteStart,
        obs: &PitchObservation,
        evidence: Evidence,
    ) -> MusicalNoteEvent {
        let id = self.next_id;
        self.next_id += 1;
        self.pending = None;
        let settling_until = (start_kind == NoteStart::Attack && self.config.settle_s > 0.0)
            .then(|| self.last_onset_s.unwrap_or(start_s) + self.config.settle_s);
        let reading = self.reading_for(sounding);
        let event = MusicalNoteEvent {
            id,
            start_s,
            end_s: None,
            start_kind,
            frequency_hz: reading.frequency_hz,
            sounding_pitch: reading.sounding_pitch,
            player_pitch: reading.player_pitch,
            sounding: reading.sounding,
            player: reading.player,
            confidence: evidence.confidence_sum / evidence.pitches.len().max(1) as f64,
            peak_level_db: evidence.peak_level_db,
            observations: evidence.pitches.len() as u32,
            course: None,
        };
        self.active = Some(Active {
            event: event.clone(),
            held,
            smoothed: sounding,
            pitches: evidence.pitches,
            confidence_sum: evidence.confidence_sum,
            invalid_run: 0,
            change_candidate: None,
            last_confidence: obs.confidence,
            last_valid_s: obs.time_s,
            settling_until,
        });
        event
    }

    fn finish_active(&mut self, end_s: f64) -> Option<MusicalNoteEvent> {
        let a = self.active.take()?;
        let mut pitches = a.pitches;
        pitches.sort_by(f64::total_cmp);
        let median = Pitch(pitches[pitches.len() / 2]);
        let reading = self.reading_for(median);
        let mut event = a.event;
        event.end_s = Some(end_s.max(event.start_s));
        self.last_end_s = event.end_s;
        event.frequency_hz = reading.frequency_hz;
        event.sounding_pitch = reading.sounding_pitch;
        event.player_pitch = reading.player_pitch;
        // Keep the held note name; report deviation from it.
        event.player = NoteMatch {
            deviation_cents: a.held.target.cents_to(reading.player_pitch),
            ..a.held
        };
        let sounding_target = self.context.transposition.player_to_sounding(a.held.target);
        event.sounding = NoteMatch {
            target: sounding_target,
            deviation_cents: sounding_target.cents_to(median),
            note: self.context.tuning.nearest(sounding_target).note,
        };
        event.confidence = a.confidence_sum / event.observations.max(1) as f64;
        Some(event)
    }

    fn reading_for(&self, sounding: Pitch) -> NoteReading {
        let freq = sounding.to_frequency(self.context.reference);
        let player_pitch = self.context.transposition.sounding_to_player(sounding);
        NoteReading {
            frequency_hz: freq,
            sounding_pitch: sounding,
            player_pitch,
            sounding: self.context.tuning.nearest(sounding),
            player: self.context.tuning.nearest(player_pitch),
        }
    }

    fn live_note(&self, a: &Active) -> LiveNote {
        let sounding = a.smoothed;
        let player_pitch = self.context.transposition.sounding_to_player(sounding);
        let held_sounding = self.context.transposition.player_to_sounding(a.held.target);
        let reading = NoteReading {
            frequency_hz: sounding.to_frequency(self.context.reference),
            sounding_pitch: sounding,
            player_pitch,
            player: NoteMatch {
                deviation_cents: a.held.target.cents_to(player_pitch),
                ..a.held
            },
            sounding: NoteMatch {
                note: self.context.tuning.nearest(held_sounding).note,
                target: held_sounding,
                deviation_cents: held_sounding.cents_to(sounding),
            },
        };
        LiveNote {
            event_id: a.event.id,
            reading,
            confidence: a.last_confidence,
            start_s: a.event.start_s,
            holding: a.invalid_run > 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::SpelledNote;
    use crate::pitch::{IntervalUnit, ReferencePitch, Transposition};

    const HOP: f64 = 512.0 / 48_000.0;
    const WIN: f64 = 4096.0 / 48_000.0;

    fn note(s: &str) -> SpelledNote {
        s.parse().unwrap()
    }

    fn voiced(i: usize, pitch: f64) -> PitchObservation {
        PitchObservation {
            time_s: i as f64 * HOP,
            window_s: WIN,
            status: ObservationStatus::Voiced,
            frequency_hz: Some(Pitch(pitch).to_frequency(ReferencePitch::default())),
            confidence: 0.95,
            level_db: -20.0,
        }
    }

    fn silent(i: usize) -> PitchObservation {
        PitchObservation {
            time_s: i as f64 * HOP,
            window_s: WIN,
            status: ObservationStatus::Silence,
            frequency_hz: None,
            confidence: 0.0,
            level_db: -80.0,
        }
    }

    fn tracker() -> NoteTracker {
        NoteTracker::new(TrackerConfig::default(), PitchContext::default())
    }

    fn shown(u: &TrackerUpdate) -> Option<SpelledNote> {
        match &u.state {
            LiveState::Note(n) => Some(n.reading.player.note),
            _ => None,
        }
    }

    #[test]
    fn silence_shows_nothing() {
        let mut t = tracker();
        for i in 0..10 {
            let u = t.update(&silent(i), None);
            assert_eq!(u.state, LiveState::Silent);
            assert!(u.started.is_none());
        }
    }

    #[test]
    fn note_confirms_after_agreeing_frames() {
        let mut t = tracker();
        let u = t.update(&voiced(0, 62.03), None);
        assert_eq!(u.state, LiveState::Listening);
        let u = t.update(&voiced(1, 61.98), None);
        let started = u.started.as_ref().expect("started");
        assert_eq!(started.player.note, note("Re4"));
        assert_eq!(shown(&u), Some(note("Re4")));
    }

    #[test]
    fn low_confidence_never_produces_a_note() {
        let mut t = tracker();
        for i in 0..20 {
            let mut o = voiced(i, 62.0);
            o.confidence = 0.5;
            let u = t.update(&o, None);
            assert!(shown(&u).is_none());
        }
    }

    #[test]
    fn boundary_jitter_does_not_flicker() {
        // Pitch hovering around the Mi4 / Mi half-sharp 4 boundary (64.25).
        let mut t = tracker();
        let mut names = Vec::new();
        for i in 0..40 {
            let p = if i % 2 == 0 { 64.22 } else { 64.28 };
            if let Some(n) = shown(&t.update(&voiced(i, p), None)) {
                names.push(n);
            }
        }
        names.dedup();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    #[test]
    fn legato_change_switches_after_change_frames() {
        let mut t = tracker();
        for i in 0..10 {
            t.update(&voiced(i, 62.0), None);
        }
        let mut switched_at = None;
        for i in 10..20 {
            let u = t.update(&voiced(i, 63.5), None);
            if let Some(s) = u.started {
                assert_eq!(s.player.note, note("Mi half-flat 4"));
                assert_eq!(s.start_kind, NoteStart::PitchChange);
                let ended = u.ended.expect("previous note ended");
                assert_eq!(ended.player.note, note("Re4"));
                switched_at = Some(i);
                break;
            }
        }
        // Median of 5 needs 3 new values to flip, then change_frames = 3.
        let at = switched_at.expect("switched");
        assert!(at <= 10 + 3 + 3, "switched at {at}");
    }

    #[test]
    fn onset_splits_repeated_same_note() {
        let mut t = tracker();
        let mut started = 0;
        let mut ended = 0;
        let mut i = 0;
        for _pluck in 0..4 {
            let onset_time = i as f64 * HOP;
            for k in 0..20 {
                let onset = (k == 0).then_some(onset_time);
                let u = t.update(&voiced(i, 67.0), onset);
                started += u.started.is_some() as u32;
                ended += u.ended.is_some() as u32;
                i += 1;
            }
        }
        assert_eq!(started, 4);
        assert_eq!(ended, 3);
        assert!(t.flush(i as f64 * HOP).is_some());
    }

    #[test]
    fn mixed_windows_after_onset_are_ignored() {
        let mut t = tracker();
        for i in 0..10 {
            t.update(&voiced(i, 62.0), None);
        }
        // Onset at frame 10; windows centred within WIN/2 after the onset
        // still contain mostly the old note: feed them the old pitch.
        let onset_t = 10.0 * HOP;
        let u = t.update(&voiced(10, 62.0), Some(onset_t));
        assert!(u.ended.is_some());
        let mut first_new = None;
        for i in 11..40 {
            let center = i as f64 * HOP;
            let p = if center - WIN / 2.0 < onset_t - WIN * 0.5 {
                62.0
            } else {
                67.0
            };
            let u = t.update(&voiced(i, p), None);
            if let Some(s) = u.started {
                first_new = Some(s);
                break;
            }
        }
        let s = first_new.expect("new note");
        assert_eq!(s.player.note, note("Sol4"));
        assert_eq!(s.start_kind, NoteStart::Attack);
        assert!((s.start_s - onset_t).abs() < 1e-9);
    }

    #[test]
    fn a_note_never_starts_before_the_previous_one_ended() {
        // Measured on real oud recordings: a note started by an attack ends
        // after a short dropout, and the next pitch (still within 0.5 s of
        // the same attack) was given the attack time as its start, so the
        // two events overlapped.
        let mut t = tracker();
        let onset_t = 10.0 * HOP;
        let mut events = Vec::new();
        for i in 10..20 {
            let onset = (i == 10).then_some(onset_t);
            let u = t.update(&voiced(i, 62.0), onset);
            events.extend(u.started);
        }
        for i in 20..27 {
            let u = t.update(&silent(i), None);
            events.extend(u.ended);
        }
        for i in 27..32 {
            let u = t.update(&voiced(i, 50.0), None);
            events.extend(u.started);
        }
        let first_end = events
            .iter()
            .find_map(|e| e.end_s)
            .expect("first note ended");
        let second = events
            .iter()
            .rfind(|e| e.end_s.is_none() && e.player.note == note("Re3"))
            .expect("second note");
        assert_eq!(second.start_kind, NoteStart::Attack);
        assert!(
            second.start_s >= first_end,
            "{second:?} overlaps {first_end}"
        );
    }

    #[test]
    fn short_dropouts_are_bridged_long_ones_end_the_note() {
        let mut t = tracker();
        for i in 0..10 {
            t.update(&voiced(i, 69.0), None);
        }
        for i in 10..13 {
            let u = t.update(&silent(i), None);
            match &u.state {
                LiveState::Note(n) => assert!(n.holding),
                s => panic!("dropped during bridge: {s:?}"),
            }
        }
        for i in 13..20 {
            t.update(&voiced(i, 69.0), None);
        }
        let mut ended = None;
        for i in 20..30 {
            if let Some(e) = t.update(&silent(i), None).ended {
                ended = Some(e);
                break;
            }
        }
        let e = ended.expect("ended");
        assert_eq!(e.player.note, note("La4"));
        assert!(e.end_s.unwrap() > e.start_s);
        assert_eq!(e.observations, 17);
    }

    #[test]
    fn events_carry_both_identities_under_transposition() {
        let ctx = PitchContext {
            transposition: Transposition::new(-2.0, IntervalUnit::Semitone),
            ..PitchContext::default()
        };
        let mut t = NoteTracker::new(TrackerConfig::default(), ctx);
        let mut started = None;
        for i in 0..5 {
            if let Some(s) = t.update(&voiced(i, 60.0), None).started {
                started = Some(s);
            }
        }
        let s = started.unwrap();
        assert_eq!(s.sounding.note, note("Do4"));
        assert_eq!(s.player.note, note("Re4"));
        let e = t.flush(1.0).unwrap();
        assert_eq!(e.sounding.note, note("Do4"));
        assert_eq!(e.player.note, note("Re4"));
    }

    #[test]
    fn displayed_deviation_is_relative_to_held_note() {
        let mut t = tracker();
        for i in 0..10 {
            t.update(&voiced(i, 64.0), None);
        }
        // Drift up to +22 cents: still Mi4, deviation ≈ +22.
        let mut last = None;
        for i in 10..30 {
            last = Some(t.update(&voiced(i, 64.22), None));
        }
        let LiveState::Note(n) = last.unwrap().state else {
            panic!()
        };
        assert_eq!(n.reading.player.note, note("Mi4"));
        assert!((n.reading.player.deviation_cents - 22.0).abs() < 0.5);
    }

    /// One pluck at frame `from`; `reading(k)` gives (pitch, level dB) `k`
    /// frames after the attack. Returns every update.
    fn pluck(
        t: &mut NoteTracker,
        from: usize,
        frames: usize,
        reading: impl Fn(usize) -> (f64, f64),
    ) -> Vec<TrackerUpdate> {
        (0..frames)
            .map(|k| {
                let (pitch, level_db) = reading(k);
                let obs = PitchObservation {
                    level_db,
                    ..voiced(from + k, pitch)
                };
                t.update(&obs, (k == 0).then_some(from as f64 * HOP))
            })
            .collect()
    }

    fn with(config: TrackerConfig) -> NoteTracker {
        NoteTracker::new(config, PitchContext::default())
    }

    #[test]
    fn a_flat_attack_renames_the_note_instead_of_splitting_it() {
        // 40 cents flat for the first 64 ms of the pluck, then Sol4.
        let flat = |k: usize| (if k < 6 { 66.6 } else { 67.0 }, -20.0);
        let mut t = tracker();
        let updates = pluck(&mut t, 0, 40, flat);
        let started: Vec<_> = updates.iter().filter_map(|u| u.started.clone()).collect();
        let revised: Vec<_> = updates.iter().filter_map(|u| u.revised.clone()).collect();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].player.note, note("Sol half-flat 4"));
        assert_eq!(revised.len(), 1);
        assert_eq!(revised[0].id, started[0].id);
        assert_eq!(revised[0].player.note, note("Sol4"));
        assert_eq!(shown(updates.last().unwrap()), Some(note("Sol4")));
        let event = t.flush(40.0 * HOP).unwrap();
        assert_eq!(event.player.note, note("Sol4"));
        assert_eq!(event.start_s, 0.0);
        assert!(updates.iter().all(|u| u.ended.is_none()));

        // Without settling the transient becomes a note of its own.
        let mut t = with(TrackerConfig {
            settle_s: 0.0,
            ..TrackerConfig::default()
        });
        let updates = pluck(&mut t, 0, 40, flat);
        assert_eq!(updates.iter().filter(|u| u.started.is_some()).count(), 2);
    }

    #[test]
    fn a_harmonic_reading_mid_note_is_skipped() {
        // Sol3 decaying; for 70 ms the detector reads its octave.
        let reading = |k: usize| {
            let level = -20.0 - k as f64 * 0.2;
            (if (40..47).contains(&k) { 67.0 } else { 55.0 }, level)
        };
        let mut t = tracker();
        let updates = pluck(&mut t, 0, 80, reading);
        assert_eq!(updates.iter().filter(|u| u.started.is_some()).count(), 1);
        assert!(updates[2..].iter().all(|u| shown(u) == Some(note("Sol3"))));

        let mut t = with(TrackerConfig {
            harmonic_guard: false,
            ..TrackerConfig::default()
        });
        let updates = pluck(&mut t, 0, 80, reading);
        assert_eq!(updates.iter().filter(|u| u.started.is_some()).count(), 3);
    }

    #[test]
    fn an_octave_at_full_level_is_a_new_note() {
        // A pluck the attack detector missed brings the level back up.
        let reading = |k: usize| {
            if k < 40 {
                (55.0, -20.0 - k as f64 * 0.2)
            } else {
                (67.0, -19.0)
            }
        };
        let mut t = tracker();
        let updates = pluck(&mut t, 0, 60, reading);
        assert_eq!(updates.iter().filter(|u| u.started.is_some()).count(), 2);
        assert_eq!(shown(updates.last().unwrap()), Some(note("Sol4")));
    }
}
