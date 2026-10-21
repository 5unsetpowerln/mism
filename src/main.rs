mod atomic_fs;
mod cache_directory;
mod command_line;
mod content_hash;
mod datapack_compatibility;
mod datapack_zip;
mod fetch_source;
mod fetched_file_cache;
mod file_name;
mod folia;
mod http_client;
mod interruption;
mod jar_cache;
mod minecraft_version;
mod modrinth_api;
mod modrinth_source;
mod output;
mod papermc;
mod papermc_api;
mod plugin_compatibility;
mod plugin_jar;
mod prompt;
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
    #[error(transparent)]
    Plugin(#[from] subcommands::plugin::Error),
    #[error(transparent)]
    Datapack(#[from] subcommands::datapack::Error),
}

impl Error {
    fn hint(&self) -> Option<String> {
        match self {
            Error::Interruption(error) => error.hint(),
            Error::New(error) => error.hint(),
            Error::Versions(error) => error.hint(),
            Error::Plugin(error) => error.hint(),
            Error::Datapack(error) => error.hint(),
        }
    }

    fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interruption(_) => None,
            Error::New(error) => error.interrupted_by(),
            Error::Versions(error) => error.interrupted_by(),
            Error::Plugin(error) => error.interrupted_by(),
            Error::Datapack(error) => error.interrupted_by(),
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
        Subcommand::Plugin(arguments) => subcommands::plugin::run(&arguments)?,
        Subcommand::Datapack(arguments) => subcommands::datapack::run(&arguments)?,
    }
    Ok(())
}

/// Prints the error and returns the exit status it calls for.
fn report(error: &Error) -> ExitCode {
    match error.interrupted_by() {
        Some(signal) => {
            write_report(&format!("interrupted by {signal}"));
            ExitCode::from(
                u8::try_from(signal.exit_status()).expect("130 and 143 fit in an exit status"),
            )
        }
        None => {
            write_report(&error_report(error));
            ExitCode::FAILURE
        }
    }
}

/// What failed, then each cause from the outermost one, then the remedy if there is one.
fn error_report(error: &Error) -> String {
    let mut lines = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        lines.push(format!("caused by: {cause}"));
        source = cause.source();
    }
    lines.extend(error.hint().map(|hint| format!("hint: {hint}")));
    lines.join("\n")
}

fn write_report(report: &str) {
    // A report that cannot be written to stdout has nowhere else to go; the exit status still
    // tells that mism failed.
    let _ = output::write_to_stdout(report);
}
