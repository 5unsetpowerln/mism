use super::CommonStepError;
use crate::command_line::ListArguments;
use crate::interruption::Signal;
use crate::output::{self, Field};
use crate::plugin_cache::{self, CachedJar};
use crate::plugin_source;
use crate::server_directory::plugins::{self, PluginState};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Plugins(#[from] plugins::Error),
    #[error(transparent)]
    PluginCache(#[from] plugin_cache::Error),
    #[error(transparent)]
    CommonStep(#[from] CommonStepError),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Plugins(error) => error.hint(),
            Error::PluginCache(error) => error.hint(),
            Error::CommonStep(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::PluginCache(error) => error.interrupted_by(),
            Error::CommonStep(error) => error.interrupted_by(),
            Error::Plugins(_) | Error::Output(_) => None,
        }
    }
}

/// `mism plugin list`, or with `--global` the plugins in the cache directory.
pub fn run(arguments: &ListArguments) -> Result<(), Error> {
    if arguments.global {
        write_cached_plugins()
    } else {
        write_server_plugins()
    }
}

/// list-1, list-2: the records and the jars of `plugins/`, held against each other.
fn write_server_plugins() -> Result<(), Error> {
    let server = super::required_target_server()?;
    let lines: Vec<String> = plugins::reconcile(&server)?
        .iter()
        .map(server_plugin_line)
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

fn server_plugin_line(plugin: &PluginState) -> String {
    let state = plugin.state.to_string();
    let source = plugin.source.as_deref();
    output::record(&[
        Field::Value(Some(&state)),
        Field::Value(Some(&plugin.name)),
        Field::Value(source.and_then(plugin_source::version_number_of)),
        Field::Value(Some(&plugin.file_name)),
        Field::Value(source),
    ])
}

/// list-3, list-4: in the order of the plugin name, then the SHA-256 (d-list-1).
fn write_cached_plugins() -> Result<(), Error> {
    let lines: Vec<String> = plugin_cache::list()?
        .iter()
        .map(cached_plugin_line)
        .collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

fn cached_plugin_line(cached_jar: &CachedJar) -> String {
    output::record(&[
        Field::Value(Some(&cached_jar.name)),
        Field::Value(plugin_source::version_number_of(&cached_jar.source)),
        Field::Value(Some(&cached_jar.sha256)),
        Field::Value(Some(&cached_jar.source)),
    ])
}
