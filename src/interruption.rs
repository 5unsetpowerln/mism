use std::fmt;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};

use signal_hook::consts::{SIGINT, SIGTERM};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to register the handler for {signal}")]
    RegisterHandler {
        signal: Signal,
        #[source]
        source: io::Error,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::RegisterHandler { .. } => None,
        }
    }
}

/// A signal that arrived before a subcommand began a change that it finishes once begun.
#[derive(Debug, thiserror::Error)]
#[error("interrupted by {0}")]
pub struct Interrupted(pub Signal);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Interrupt,
    Terminate,
}

impl Signal {
    const ALL: [Signal; 2] = [Signal::Interrupt, Signal::Terminate];

    fn number(self) -> i32 {
        match self {
            Signal::Interrupt => SIGINT,
            Signal::Terminate => SIGTERM,
        }
    }

    fn flag_value(self) -> usize {
        usize::try_from(self.number()).expect("signal numbers are positive")
    }

    /// The shell convention for a process ended by a signal: 128 plus the signal number.
    pub fn exit_status(self) -> i32 {
        128 + self.number()
    }
}

impl fmt::Display for Signal {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            Signal::Interrupt => "SIGINT",
            Signal::Terminate => "SIGTERM",
        })
    }
}

const NO_SIGNAL: usize = 0;

/// The `flag_value` of the signal that arrived last, written by the handlers and never reset.
static RECEIVED_SIGNAL: LazyLock<Arc<AtomicUsize>> =
    LazyLock::new(|| Arc::new(AtomicUsize::new(NO_SIGNAL)));

/// Makes SIGINT and SIGTERM only record their arrival instead of ending the process.
pub fn register_handlers() -> Result<(), Error> {
    Signal::ALL.into_iter().try_for_each(register_handler)
}

fn register_handler(signal: Signal) -> Result<(), Error> {
    signal_hook::flag::register_usize(
        signal.number(),
        Arc::clone(&RECEIVED_SIGNAL),
        signal.flag_value(),
    )
    .map(drop)
    .map_err(|source| Error::RegisterHandler { signal, source })
}

pub fn received() -> Option<Signal> {
    let value = RECEIVED_SIGNAL.load(Ordering::SeqCst);
    Signal::ALL
        .into_iter()
        .find(|signal| signal.flag_value() == value)
}

/// Checked right before a subcommand starts to change the cache directory, where it places files,
/// or `mism.toml`, and never after it started (d-pcmd-1, d-pcmd-2).
pub fn fail_if_received() -> Result<(), Interrupted> {
    fail_if(received())
}

fn fail_if(received: Option<Signal>) -> Result<(), Interrupted> {
    match received {
        Some(signal) => Err(Interrupted(signal)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// d-pcmd-1: a signal that arrived stops the subcommand before it changes anything.
    #[test]
    fn received_signal_stops_before_changes() {
        assert!(matches!(
            fail_if(Some(Signal::Terminate)),
            Err(Interrupted(Signal::Terminate))
        ));
    }

    /// d-pcmd-2: a signal that arrived stops a datapack subcommand before it changes the world
    /// directory or `mism.toml`, with the same check.
    #[test]
    fn received_signal_stops_a_datapack_subcommand_before_changes() {
        assert!(matches!(
            fail_if(Some(Signal::Interrupt)),
            Err(Interrupted(Signal::Interrupt))
        ));
    }

    /// d-pcmd-1: without a signal the subcommand goes on.
    #[test]
    fn no_signal_lets_the_subcommand_go_on() {
        assert!(fail_if(None).is_ok());
    }
}
