use crate::command_line::PluginSearchArguments;
use crate::interruption::Signal;
use crate::minecraft_version;
use crate::modrinth_api::response::Version;
use crate::modrinth_source;
use crate::output::{self, Field};
use crate::subcommands::modrinth_search::{self, Found};

/// The search filter of plugins (srch-1).
const FACETS: &str = r#"[["project_type:plugin"]]"#;
/// The server software mism manages, as Modrinth names them in `loaders` (srch-2).
const SERVER_SOFTWARE: [&str; 2] = ["paper", "folia"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Search(#[from] modrinth_search::Error),
    #[error(transparent)]
    Output(#[from] output::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Search(error) => error.hint(),
            Error::Output(error) => error.hint(),
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Search(error) => error.interrupted_by(),
            Error::Output(_) => None,
        }
    }
}

/// `mism plugin search` (srch-1 to srch-5). It writes the plugins only after every request
/// succeeded (srch-4).
pub fn run(arguments: &PluginSearchArguments) -> Result<(), Error> {
    let found = modrinth_search::search(
        &arguments.term,
        arguments.limit.as_deref(),
        FACETS,
        is_for_server_software,
    )?;
    write_found(&found)
}

/// Whether a version is for Paper or Folia, so that it describes a plugin found (srch-2).
fn is_for_server_software(version: &Version) -> bool {
    version
        .loaders
        .iter()
        .any(|loader| SERVER_SOFTWARE.contains(&loader.as_str()))
}

/// Step 5: one record per plugin found, in Modrinth's order (srch-1, srch-2, srch-3, plg-9).
fn write_found(found: &[Found]) -> Result<(), Error> {
    let lines: Vec<String> = found.iter().map(found_line).collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

/// The identifier, the plugin name, and the version number, the server software, and the newest
/// Minecraft version of the version found, each `-` without it (srch-2, plg-9).
fn found_line(found: &Found) -> String {
    let source = modrinth_source::source_text(&found.hit.slug, &[]);
    let version = found.version.as_ref();
    output::record(&[
        Field::Value(Some(&source)),
        Field::Value(Some(&found.hit.title)),
        Field::Value(version.map(|version| version.version_number.as_str())),
        Field::List(version.map_or_else(Vec::new, |version| {
            version.loaders.iter().map(String::as_str).collect()
        })),
        Field::Value(version.and_then(|version| minecraft_version::newest(&version.game_versions))),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version_for(loaders: &[&str]) -> Version {
        Version {
            id: "v".to_owned(),
            version_number: "1.0".to_owned(),
            version_type: "release".to_owned(),
            loaders: loaders.iter().map(|loader| (*loader).to_owned()).collect(),
            game_versions: vec!["26.3".to_owned()],
            date_published: "2026-10-21T00:00:00Z".parse().unwrap(),
            dependencies: Vec::new(),
            files: Vec::new(),
        }
    }

    /// srch-2: the search describes plugins by versions for Paper or Folia only.
    #[test]
    fn plugin_search_hits_are_described_by_versions_for_paper_or_folia() {
        let described: Vec<_> = [&["folia"][..], &["bukkit", "paper"], &["fabric"]]
            .into_iter()
            .map(|loaders| is_for_server_software(&version_for(loaders)))
            .collect();
        assert_eq!(described, [true, true, false]);
    }
}
