use std::io::{self, Write};
use std::path::PathBuf;

use super::project_name;
use crate::command_line::NewArguments;
use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;
use crate::jar_cache;
use crate::papermc_api::selection::{self, Selected};
use crate::papermc_api::{self, HttpPaperMcApi};
use crate::server_directory::ServerDirectory;
use crate::server_directory::creation::{self, CachedJar};
use crate::server_directory::mism_toml::MismToml;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Creation(#[from] creation::Error),
    #[error(transparent)]
    HttpClient(#[from] http_client::Error),
    #[error(transparent)]
    PaperMcApi(#[from] papermc_api::Error),
    #[error(transparent)]
    JarCache(#[from] jar_cache::Error),
    #[error("failed to write the created server directory to stdout")]
    WriteStdout(#[source] io::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Creation(error) => error.hint(),
            Error::HttpClient(error) => error.hint(),
            Error::PaperMcApi(error) => error.hint(),
            Error::JarCache(error) => error.hint(),
            Error::WriteStdout(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Creation(error) => error.interrupted_by(),
            Error::HttpClient(error) => error.interrupted_by(),
            Error::PaperMcApi(error) => error.interrupted_by(),
            Error::JarCache(error) => error.interrupted_by(),
            Error::WriteStdout(_) => None,
        }
    }
}

/// The steps of 「mism new の手順と中断」: 1 to 3 in `creation::check_target_path`, 4 in
/// `select_versions` and `cache_jar`, 5 to 8 in `creation::create_server_directory`, 9 in
/// `print_server_directory`.
pub fn run(arguments: &NewArguments) -> Result<(), Error> {
    // The path is checked before any network access (d-new-5).
    let target = creation::check_target_path(&arguments.path)?;
    let client = HttpClient::new()?;
    let project = project_name(arguments.software);
    let selected = select_versions(&client, project, arguments.version.as_deref())?;
    let cached_jar = cache_jar(&client, project, &selected)?;
    let mism_toml = MismToml {
        software: arguments.software.to_string(),
        minecraft_version: selected.minecraft_version.clone(),
        software_version: selected.software_version,
    };
    let server_directory = creation::create_server_directory(
        target,
        &CachedJar {
            path: &cached_jar,
            name: &selected.download.name,
            sha256: &selected.download.sha256,
        },
        mism_toml,
        selected.minimum_java_version,
    )?;
    print_server_directory(&server_directory)
}

/// Step 4: the Minecraft version, the software version, and its download.
fn select_versions(
    client: &HttpClient,
    project: &str,
    requested: Option<&str>,
) -> Result<Selected, Error> {
    let api = HttpPaperMcApi::new(client.clone());
    Ok(selection::select(&api, project, requested)?)
}

/// Step 4: the path of the verified jar in the cache directory.
fn cache_jar(client: &HttpClient, project: &str, selected: &Selected) -> Result<PathBuf, Error> {
    Ok(jar_cache::obtain(
        client,
        project,
        &selected.minecraft_version,
        selected.software_version,
        &selected.download.name,
        &selected.download.sha256,
        &selected.download.url,
    )?)
}

/// Step 9 (new-10).
fn print_server_directory(server_directory: &ServerDirectory) -> Result<(), Error> {
    let mism_toml = &server_directory.mism_toml;
    writeln!(
        io::stdout(),
        "{} {} {} {}",
        mism_toml.software,
        mism_toml.minecraft_version,
        mism_toml.software_version,
        server_directory.path.display()
    )
    .map_err(Error::WriteStdout)
}
