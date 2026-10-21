pub mod creation;
pub mod datapacks;
mod initial_launch;
pub mod mism_toml;
pub mod placed_files;
pub mod plugins;
pub mod server_jar;
mod server_properties;

use std::env;
use std::io;
use std::path::{Path, PathBuf};

use mism_toml::MismToml;

const MISM_TOML_FILE_NAME: &str = "mism.toml";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to determine the current directory")]
    CurrentDirectory(#[source] io::Error),
    #[error(transparent)]
    MismToml(#[from] mism_toml::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::CurrentDirectory(_) => None,
            Error::MismToml(error) => error.hint(),
        }
    }
}

/// A server directory: where it is, and what its `mism.toml` records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerDirectory {
    pub path: PathBuf,
    pub mism_toml: MismToml,
}

impl ServerDirectory {
    /// The current directory as the target server, or `None` when it has no `mism.toml`
    /// (plg-1, plg-2, dp-1, dp-2).
    pub fn current() -> Result<Option<ServerDirectory>, Error> {
        ServerDirectory::at(env::current_dir().map_err(Error::CurrentDirectory)?)
    }

    fn at(path: PathBuf) -> Result<Option<ServerDirectory>, Error> {
        match MismToml::read(&mism_toml_path(&path)) {
            Ok(mism_toml) => Ok(Some(ServerDirectory { path, mism_toml })),
            Err(error) if error.is_not_found() => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn mism_toml_path(&self) -> PathBuf {
        mism_toml_path(&self.path)
    }
}

fn mism_toml_path(directory: &Path) -> PathBuf {
    directory.join(MISM_TOML_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    const PAPER_26_3_206: &str =
        "software = \"paper\"\nminecraft_version = \"26.3\"\nsoftware_version = 206\n";

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    /// plg-1, plg-2: a directory without `mism.toml` is no target server.
    #[test]
    fn directory_without_mism_toml_is_no_server() {
        let directory = temporary_directory();

        assert_eq!(
            ServerDirectory::at(directory.path().to_path_buf()).unwrap(),
            None
        );
    }

    /// plg-1, plg-2: a directory with `mism.toml` is the target server.
    #[test]
    fn directory_with_mism_toml_is_the_server() {
        let directory = temporary_directory();
        fs::write(directory.path().join("mism.toml"), PAPER_26_3_206).unwrap();

        let server = ServerDirectory::at(directory.path().to_path_buf())
            .unwrap()
            .unwrap();

        assert_eq!(
            (server.path, server.mism_toml.minecraft_version),
            (directory.path().to_path_buf(), "26.3".to_owned())
        );
    }

    /// 「mism plugin と mism datapack の対象のサーバーと共通の手順」 方法 1: an invalid `mism.toml`
    /// is an error.
    #[test]
    fn invalid_mism_toml_is_an_error() {
        let directory = temporary_directory();
        fs::write(directory.path().join("mism.toml"), "software = \"paper\"\n").unwrap();

        assert!(ServerDirectory::at(directory.path().to_path_buf()).is_err());
    }
}
