use crate::command_line::DatapackSearchArguments;
use crate::interruption::Signal;
use crate::minecraft_version;
use crate::modrinth_source;
use crate::output::{self, Field};
use crate::subcommands::modrinth_search::{self, Found};

/// The search filter of datapacks (dsrch-1).
const FACETS: &str = r#"[["project_type:datapack"]]"#;

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

/// `mism datapack search` (dsrch-1 to dsrch-5). It writes the datapacks only after every request
/// succeeded (dsrch-4).
pub fn run(arguments: &DatapackSearchArguments) -> Result<(), Error> {
    let found = modrinth_search::search(
        &arguments.term,
        arguments.limit.as_deref(),
        FACETS,
        super::is_datapack_version,
    )?;
    write_found(&found)
}

/// Step 5: one record per datapack found, in Modrinth's order (dsrch-1, dsrch-2, dsrch-3, dp-12).
fn write_found(found: &[Found]) -> Result<(), Error> {
    let lines: Vec<String> = found.iter().map(found_line).collect();
    Ok(output::write_to_stdout(&lines.join("\n"))?)
}

/// The identifier, the datapack name, and the version number and the newest Minecraft version of
/// the datapack version found, each `-` without it (dsrch-2, dp-12).
fn found_line(found: &Found) -> String {
    let source = modrinth_source::source_text(&found.hit.slug, &[]);
    let version = found.version.as_ref();
    output::record(&[
        Field::Value(Some(&source)),
        Field::Value(Some(&found.hit.title)),
        Field::Value(version.map(|version| version.version_number.as_str())),
        Field::Value(version.and_then(|version| minecraft_version::newest(&version.game_versions))),
    ])
}
