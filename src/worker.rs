use crate::{
    config::Config,
    limits::RateLimits,
    monitor::{Failure, retry_delay},
    provider::Provider,
};
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Debug)]
pub enum Event {
    Started,
    Success(RateLimits, String),
    Failed(Failure),
}
pub enum Command {
    Refresh,
    Configure(Config),
    Shutdown,
}
pub struct Worker {
    pub commands: Sender<Command>,
    join: Option<JoinHandle<()>>,
    cancellation: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Worker {
    pub fn start(config: Config, notify: impl Fn() + Send + 'static) -> (Self, Receiver<Event>) {
        let (commands, requests) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        let cancellation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled = cancellation.clone();
        let join = thread::spawn(move || {
            let mut config = config;
            let mut provider =
                Provider::new(config.codex_path.clone()).with_cancellation(cancelled.clone());
            let mut failures = 0;
            'worker: loop {
                let _ = events.send(Event::Started);
                notify();
                let event = match provider.read() {
                    Ok((limits, source)) => {
                        failures = 0;
                        Event::Success(limits, source)
                    }
                    Err(f) => {
                        failures += 1;
                        Event::Failed(f)
                    }
                };
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                if events.send(event).is_err() {
                    break;
                }
                notify();
                let timeout = Duration::from_secs(retry_delay(config.interval_secs, failures));
                match requests.recv_timeout(timeout) {
                    Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                    Ok(Command::Configure(c)) => {
                        provider = Provider::new(c.codex_path.clone())
                            .with_cancellation(cancelled.clone());
                        config = c;
                        failures = 0;
                    }
                    Ok(Command::Refresh) => {
                        failures = 0;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
                // Coalesce queued changes/refreshes, never run concurrent queries.
                while let Ok(command) = requests.try_recv() {
                    match command {
                        Command::Shutdown => break 'worker,
                        Command::Configure(c) => {
                            provider = Provider::new(c.codex_path.clone())
                                .with_cancellation(cancelled.clone());
                            config = c;
                            failures = 0;
                        }
                        Command::Refresh => {
                            failures = 0;
                        }
                    }
                }
            }
        });
        (
            Self {
                commands,
                join: Some(join),
                cancellation,
            },
            receiver,
        )
    }
    pub fn stop(&mut self) {
        self.cancellation
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.commands.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}
