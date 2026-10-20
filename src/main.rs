mod atomic_fs;
mod command_line;
mod folia;
mod http_client;
mod interruption;
mod jar_cache;
mod minecraft_version;
mod papermc;
mod papermc_api;
mod server_directory;
mod subcommands;

use std::error::Error as _;
use std::process::ExitCode;

use command_line::{CommandLine, Subcommand};
use interruption::Signal;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Interruption(#[from] interruption::Error),
    #[error(transparent)]
    New(#[from] subcommands::new::Error),
    #[error(transparent)]
    Versions(#[from] subcommands::versions::Error),
}

impl Error {
    fn hint(&self) -> Option<String> {
        match self {
            Error::Interruption(error) => error.hint(),
            Error::New(error) => error.hint(),
            Error::Versions(error) => error.hint(),
        }
    }

    fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interruption(_) => None,
            Error::New(error) => error.interrupted_by(),
            Error::Versions(error) => error.interrupted_by(),
        }
    }
}

fn main() -> ExitCode {
    match run(CommandLine::parse_or_exit()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report(&error),
    }
}

fn run(command_line: CommandLine) -> Result<(), Error> {
    interruption::register_handlers()?;
    match command_line.subcommand {
        Subcommand::New(arguments) => subcommands::new::run(&arguments)?,
        Subcommand::Versions(arguments) => subcommands::versions::run(&arguments)?,
    }
    Ok(())
}

/// Prints the error and returns the exit status it calls for.
fn report(error: &Error) -> ExitCode {
    match error.interrupted_by() {
        Some(signal) => {
            eprintln!("mmmm: interrupted by {signal}");
            ExitCode::from(
                u8::try_from(signal.exit_status()).expect("130 and 143 fit in an exit status"),
            )
        }
        None => {
            print_error(error);
            ExitCode::FAILURE
        }
    }
}

fn print_error(error: &Error) {
    eprintln!("mmmm: {error}");
    let mut source = error.source();
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
    if let Some(hint) = error.hint() {
        eprintln!("hint: {hint}");
    }
}
