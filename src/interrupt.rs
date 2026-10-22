use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGUSR1, SIGUSR2};
use signal_hook::flag;
use signal_hook::low_level::signal_name;

/// Signals that stop a run gracefully. SIGKILL cannot be caught by any process.
pub const SIGNALS: [i32; 5] = [SIGINT, SIGTERM, SIGHUP, SIGUSR1, SIGUSR2];

/// The stop request shared by every worker: the number of the last stop signal, or none.
#[derive(Clone, Debug, Default)]
pub struct Interrupt(Arc<AtomicUsize>);

impl Interrupt {
    /// An interrupt that any of [`SIGNALS`] raises.
    pub fn register() -> io::Result<Self> {
        let interrupt = Self::default();
        for signal in SIGNALS {
            flag::register_usize(signal, Arc::clone(&interrupt.0), signal as usize)?;
        }
        Ok(interrupt)
    }

    /// Stop the run as though `signal` had arrived.
    pub fn raise(&self, signal: i32) {
        self.0.store(signal as usize, Ordering::Relaxed);
    }

    /// The signal that stopped the run, if one has.
    pub fn signal(&self) -> Option<i32> {
        match self.0.load(Ordering::Relaxed) {
            0 => None,
            signal => i32::try_from(signal).ok(),
        }
    }

    pub fn is_raised(&self) -> bool {
        self.0.load(Ordering::Relaxed) != 0
    }
}

/// The conventional name of `signal`, such as `SIGINT`.
pub fn name(signal: i32) -> String {
    signal_name(signal).map_or_else(|| format!("signal {signal}"), str::to_owned)
}
