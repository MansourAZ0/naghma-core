//! A thread-safe holder for at most one running [`Engine`], for application
//! backends (Tauri commands, CLIs, …).
//!
//! The application supplies a *sink* that receives every [`EngineEvent`]
//! (typically mapping frames to its UI format and forwarding them over IPC).
//! The sink is kept across restarts, so changing the input device or
//! analysis parameters is transparent to the UI.

use crate::engine::{Engine, EngineConfig, EngineError, EngineEvent, EngineStats, StreamInfo};
use std::sync::{Arc, Mutex};

type Sink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

struct Running {
    engine: Engine,
    config: EngineConfig,
    sink: Sink,
}

/// Owner of the (optional) running engine.
#[derive(Default)]
pub struct EngineSession {
    running: Mutex<Option<Running>>,
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
}

impl EngineSession {
    /// Create an idle session.
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` while an engine is running.
    pub fn is_running(&self) -> bool {
        self.lock().is_some()
    }

    /// Start (replacing any running engine). Blocks while the OS opens the
    /// device — call from a thread that may block.
    pub fn start(
        &self,
        config: EngineConfig,
        sink: impl Fn(EngineEvent) + Send + Sync + 'static,
    ) -> Result<StreamInfo, EngineError> {
        self.start_with_sink(config, Arc::new(sink))
    }

    fn start_with_sink(&self, config: EngineConfig, sink: Sink) -> Result<StreamInfo, EngineError> {
        self.stop();
        let forward = sink.clone();
        let engine = Engine::start(config.clone(), move |e| forward(e))?;
        let info = engine.info().clone();
        *self.lock() = Some(Running {
            engine,
            config,
            sink,
        });
        Ok(info)
    }

    /// Stop the running engine, if any.
    pub fn stop(&self) {
        let running = self.lock().take();
        if let Some(r) = running {
            r.engine.stop();
        }
    }

    /// Apply a new configuration. Context and tracker changes are applied
    /// live; source or analysis changes restart the stream with the same
    /// sink.
    pub fn reconfigure(&self, config: EngineConfig) -> Result<Reconfigured, EngineError> {
        let restart = {
            let mut guard = self.lock();
            let Some(r) = guard.as_mut() else {
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
            .as_ref()
            .map(|r| (r.engine.info().clone(), r.engine.stats()))
    }

    /// The configuration of the running engine.
    pub fn config(&self) -> Option<EngineConfig> {
        self.lock().as_ref().map(|r| r.config.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Running>> {
        // A panic while holding the lock leaves plain data behind; recover.
        self.running.lock().unwrap_or_else(|e| e.into_inner())
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
}
