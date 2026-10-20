use std::cmp::Reverse;
use std::io::{self, Write};

use super::project_name;
use crate::command_line::VersionsArguments;
use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;
use crate::minecraft_version::{self, MinecraftVersion};
use crate::papermc_api::response::VersionEntry;
use crate::papermc_api::{self, HttpPaperMcApi, PaperMcApi};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    HttpClient(#[from] http_client::Error),
    #[error(transparent)]
    PaperMcApi(#[from] papermc_api::Error),
    #[error(transparent)]
    MinecraftVersion(#[from] minecraft_version::Error),
    #[error("failed to write the version list to stdout")]
    WriteStdout(#[source] io::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::HttpClient(error) => error.hint(),
            Error::PaperMcApi(error) => error.hint(),
            Error::MinecraftVersion(error) => error.hint(),
            Error::WriteStdout(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::HttpClient(error) => error.interrupted_by(),
            Error::PaperMcApi(error) => error.interrupted_by(),
            Error::MinecraftVersion(_) | Error::WriteStdout(_) => None,
        }
    }
}

pub fn run(arguments: &VersionsArguments) -> Result<(), Error> {
    let api = HttpPaperMcApi::new(HttpClient::new()?);
    let entries = api.minecraft_versions(project_name(arguments.software))?;
    let output = format_entries(sort_newest_first(entries)?);
    io::stdout()
        .write_all(output.as_bytes())
        .map_err(Error::WriteStdout)
}

fn sort_newest_first(entries: Vec<VersionEntry>) -> Result<Vec<VersionEntry>, Error> {
    let mut keyed = Vec::with_capacity(entries.len());
    for entry in entries {
        keyed.push((MinecraftVersion::parse(&entry.minecraft_version)?, entry));
    }
    keyed.sort_by_key(|(version, _)| Reverse(version.clone()));
    Ok(keyed.into_iter().map(|(_, entry)| entry).collect())
}

fn format_entries(entries: Vec<VersionEntry>) -> String {
    entries
        .iter()
        .map(|entry| format!("{}\t{}\n", entry.minecraft_version, entry.support_status))
        .collect()
}
