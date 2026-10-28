//! A thread-safe holder for at most one running [`Engine`], for application
//! backends (Tauri commands, CLIs, …).
//!
//! The application supplies a *sink* that receives every [`EngineEvent`]
//! (typically mapping frames to its UI format and forwarding them over IPC).
//! The sink is kept across restarts, so changing the input device or
//! analysis parameters is transparent to the UI.
//!
//! ## Concurrent calls
//!
//! Opening a device can block for seconds (a microphone-permission prompt),
//! and the session does not hold its lock meanwhile, so `stop`, `status` and
//! `reconfigure` stay responsive. Every `start` and `stop` takes a new
//! generation number; when a start finishes opening its device it keeps the
//! engine only if no later `start` or `stop` happened, and otherwise closes
//! it and returns [`EngineError::Superseded`]. So the last call wins: a stop
//! pressed during a slow start leaves nothing running, and two overlapping
//! starts never leave two engines. A `reconfigure` during a start is applied
//! when the device has opened ([`Reconfigured::Pending`]).

use crate::engine::{Engine, EngineConfig, EngineError, EngineEvent, EngineStats, StreamInfo};
use std::sync::{Arc, Mutex};

type Sink = Arc<dyn Fn(EngineEvent) + Send + Sync>;
type Opener = Arc<dyn Fn(EngineConfig, Sink) -> Result<Engine, EngineError> + Send + Sync>;

struct Running {
    engine: Engine,
    config: EngineConfig,
    sink: Sink,
}

#[derive(Default)]
struct State {
    running: Option<Running>,
    /// Incremented by every start and stop.
    generation: u64,
    /// Latest configuration requested while a start is opening its device.
    starting: Option<EngineConfig>,
}

/// Owner of the (optional) running engine.
pub struct EngineSession {
    state: Mutex<State>,
    open: Opener,
}

impl Default for EngineSession {
    fn default() -> Self {
        Self::with_opener(Arc::new(|config, sink: Sink| {
            Engine::start(config, move |e| sink(e))
        }))
    }
}

impl std::fmt::Debug for EngineSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineSession")
            .field("running", &self.is_running())
            .finish()
    }
}

/// What [`EngineSession::reconfigure`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconfigured {
    /// Nothing is running; the configuration will apply on next start.
    NotRunning,
    /// Naming/tracking settings were applied to the live stream.
    Live,
    /// The source or analysis changed, so the stream was restarted.
    Restarted,
    /// A start is still opening its device; the configuration is applied
    /// (live, or by opening again) when it has.
    Pending,
}

impl EngineSession {
    /// Create an idle session.
    pub fn new() -> Self {
        Self::default()
    }

    fn with_opener(open: Opener) -> Self {
        Self {
            state: Mutex::new(State::default()),
            open,
        }
    }

    /// `true` while an engine is running.
    pub fn is_running(&self) -> bool {
        self.lock().running.is_some()
    }

    /// Start (replacing any running engine). Blocks while the OS opens the
    /// device — call from a thread that may block. Returns
    /// [`EngineError::Superseded`] if a later `start` or `stop` was called
    /// before the device opened (see the module documentation).
    pub fn start(
        &self,
        config: EngineConfig,
        sink: impl Fn(EngineEvent) + Send + Sync + 'static,
    ) -> Result<StreamInfo, EngineError> {
        self.start_with_sink(config, Arc::new(sink))
    }

    fn start_with_sink(&self, config: EngineConfig, sink: Sink) -> Result<StreamInfo, EngineError> {
        let (generation, previous) = {
            let mut state = self.lock();
            state.generation += 1;
            state.starting = Some(config.clone());
            (state.generation, state.running.take())
        };
        if let Some(r) = previous {
            r.engine.stop();
        }
        let mut config = config;
        loop {
            let opened = (self.open)(config.clone(), sink.clone());
            let mut state = self.lock();
            if state.generation != generation {
                drop(state);
                if let Ok(engine) = opened {
                    engine.stop();
                }
                return Err(EngineError::Superseded);
            }
            let engine = match opened {
                Ok(engine) => engine,
                Err(e) => {
                    state.starting = None;
                    return Err(e);
                }
            };
            let wanted = state.starting.take().unwrap_or_else(|| config.clone());
            if wanted.source != config.source || wanted.analysis != config.analysis {
                // Reconfigured to another device or analysis while opening.
                state.starting = Some(wanted.clone());
                drop(state);
                engine.stop();
                config = wanted;
                continue;
            }
            if wanted.context != config.context {
                engine.set_context(wanted.context.clone());
            }
            if wanted.tracker != config.tracker {
                engine.set_tracker_config(wanted.tracker);
            }
            let info = engine.info().clone();
            state.running = Some(Running {
                engine,
                config: wanted,
                sink,
            });
            return Ok(info);
        }
    }

    /// Stop the running engine, if any, and cancel a start in progress.
    pub fn stop(&self) {
        let running = {
            let mut state = self.lock();
            state.generation += 1;
            state.starting = None;
            state.running.take()
        };
        if let Some(r) = running {
            r.engine.stop();
        }
    }

    /// Apply a new configuration. Context and tracker changes are applied
    /// live; source or analysis changes restart the stream with the same
    /// sink.
    pub fn reconfigure(&self, config: EngineConfig) -> Result<Reconfigured, EngineError> {
        let restart = {
            let mut state = self.lock();
            if state.starting.is_some() {
                state.starting = Some(config);
                return Ok(Reconfigured::Pending);
            }
            let Some(r) = state.running.as_mut() else {
                return Ok(Reconfigured::NotRunning);
            };
            if r.config.source != config.source || r.config.analysis != config.analysis {
                Some(r.sink.clone())
            } else {
                if r.config.context != config.context {
                    r.engine.set_context(config.context.clone());
                }
                if r.config.tracker != config.tracker {
                    r.engine.set_tracker_config(config.tracker);
                }
                r.config = config.clone();
                None
            }
        };
        match restart {
            Some(sink) => {
                self.start_with_sink(config, sink)?;
                Ok(Reconfigured::Restarted)
            }
            None => Ok(Reconfigured::Live),
        }
    }

    /// Stream facts and counters while running.
    pub fn status(&self) -> Option<(StreamInfo, EngineStats)> {
        self.lock()
            .running
            .as_ref()
            .map(|r| (r.engine.info().clone(), r.engine.stats()))
    }

    /// The configuration of the running engine.
    pub fn config(&self) -> Option<EngineConfig> {
        self.lock().running.as_ref().map(|r| r.config.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // A panic while holding the lock leaves plain data behind; recover.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::SourceConfig;
    use crate::simulation::SimulationSpec;
    use naghma_core::interpret::PitchContext;
    use naghma_core::pitch::{IntervalUnit, Transposition};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn sim_config() -> EngineConfig {
        EngineConfig {
            source: SourceConfig::Simulation(SimulationSpec::default()),
            analysis: Default::default(),
            tracker: Default::default(),
            context: PitchContext::default(),
        }
    }

    #[test]
    fn start_reconfigure_restart_stop() {
        let session = EngineSession::new();
        let frames = Arc::new(AtomicUsize::new(0));
        let counter = frames.clone();
        session
            .start(sim_config(), move |e| {
                if matches!(e, EngineEvent::Frame(_)) {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            })
            .unwrap();
        assert!(session.is_running());
        std::thread::sleep(Duration::from_millis(300));
        assert!(frames.load(Ordering::Relaxed) > 0);

        // Context change: applied live.
        let mut cfg = sim_config();
        cfg.context.transposition = Transposition::new(-2.0, IntervalUnit::Semitone);
        assert_eq!(
            session.reconfigure(cfg.clone()).unwrap(),
            Reconfigured::Live
        );
        assert_eq!(
            session.config().unwrap().context.transposition,
            cfg.context.transposition
        );

        // Analysis change: restart, same sink keeps receiving frames.
        cfg.analysis.silence_threshold_db = -50.0;
        assert_eq!(session.reconfigure(cfg).unwrap(), Reconfigured::Restarted);
        let before = frames.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(300));
        assert!(frames.load(Ordering::Relaxed) > before);

        session.stop();
        assert!(!session.is_running());
        assert_eq!(
            session.reconfigure(sim_config()).unwrap(),
            Reconfigured::NotRunning
        );
    }

    /// A session whose devices take `delay` to open.
    fn slow_session(delay: Duration, opened: Arc<AtomicUsize>) -> Arc<EngineSession> {
        Arc::new(EngineSession::with_opener(Arc::new(
            move |config, sink: Sink| {
                std::thread::sleep(delay);
                opened.fetch_add(1, Ordering::SeqCst);
                Engine::start(config, move |e| sink(e))
            },
        )))
    }

    #[test]
    fn stop_during_a_slow_start_leaves_nothing_running() {
        let opened = Arc::new(AtomicUsize::new(0));
        let session = slow_session(Duration::from_millis(300), opened.clone());
        let frames = Arc::new(AtomicUsize::new(0));
        let counter = frames.clone();
        let starter = {
            let session = session.clone();
            std::thread::spawn(move || {
                session.start(sim_config(), move |_| {
                    counter.fetch_add(1, Ordering::SeqCst);
                })
            })
        };
        std::thread::sleep(Duration::from_millis(100));
        session.stop(); // the user pressed Stop while the device was opening
        assert_eq!(starter.join().unwrap(), Err(EngineError::Superseded));
        assert_eq!(opened.load(Ordering::SeqCst), 1);
        assert!(!session.is_running());
        let after = frames.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            frames.load(Ordering::SeqCst),
            after,
            "the engine was closed"
        );
    }

    #[test]
    fn overlapping_starts_leave_exactly_one_engine() {
        let opened = Arc::new(AtomicUsize::new(0));
        let session = slow_session(Duration::from_millis(200), opened.clone());
        let sinks: Vec<Arc<AtomicUsize>> = (0..2).map(|_| Arc::new(AtomicUsize::new(0))).collect();
        let starters: Vec<_> = sinks
            .iter()
            .enumerate()
            .map(|(i, frames)| {
                let session = session.clone();
                let frames = frames.clone();
                std::thread::sleep(Duration::from_millis(50 * i as u64));
                std::thread::spawn(move || {
                    session.start(sim_config(), move |e| {
                        if matches!(e, EngineEvent::Frame(_)) {
                            frames.fetch_add(1, Ordering::SeqCst);
                        }
                    })
                })
            })
            .collect();
        let results: Vec<_> = starters.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results[0], Err(EngineError::Superseded));
        assert!(results[1].is_ok());
        let first = sinks[0].load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            sinks[0].load(Ordering::SeqCst),
            first,
            "superseded engine closed"
        );
        assert!(sinks[1].load(Ordering::SeqCst) > 0, "the last start runs");
        session.stop();
    }

    #[test]
    fn reconfigure_during_a_slow_start_is_applied_when_it_opens() {
        let opened = Arc::new(AtomicUsize::new(0));
        let session = slow_session(Duration::from_millis(200), opened.clone());
        let starter = {
            let session = session.clone();
            std::thread::spawn(move || session.start(sim_config(), |_| {}))
        };
        std::thread::sleep(Duration::from_millis(50));
        let mut live = sim_config();
        live.context.transposition = Transposition::new(-2.0, IntervalUnit::Semitone);
        assert_eq!(
            session.reconfigure(live.clone()).unwrap(),
            Reconfigured::Pending
        );
        starter.join().unwrap().unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), 1, "applied live, no reopen");
        assert_eq!(
            session.config().unwrap().context.transposition,
            live.context.transposition
        );

        // An analysis change while opening opens again with it.
        session.stop();
        let starter = {
            let session = session.clone();
            std::thread::spawn(move || session.start(sim_config(), |_| {}))
        };
        std::thread::sleep(Duration::from_millis(50));
        let mut other = sim_config();
        other.analysis.silence_threshold_db = -50.0;
        assert_eq!(
            session.reconfigure(other.clone()).unwrap(),
            Reconfigured::Pending
        );
        starter.join().unwrap().unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), 3);
        assert_eq!(session.config().unwrap().analysis, other.analysis);
        session.stop();
        assert_eq!(
            session.reconfigure(sim_config()).unwrap(),
            Reconfigured::NotRunning
        );
    }
}
