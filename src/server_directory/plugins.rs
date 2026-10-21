use std::path::PathBuf;

use super::ServerDirectory;
use super::mism_toml::MismToml;
use super::placed_files::{self, PlacedFiles};
use crate::plugin_jar::{self, Declaration};

const PLUGINS_DIRECTORY_NAME: &str = "plugins";
const JAR_SUFFIX: &str = ".jar";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Files(#[from] placed_files::Error),
    #[error("failed to read the declaration of {}", path.display())]
    UnreadableDeclaration {
        path: PathBuf,
        #[source]
        source: plugin_jar::Error,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::UnreadableDeclaration { path, .. } => {
                Some(format!("move {} out of plugins/", path.display()))
            }
            Error::Files(error) => error.hint(),
        }
    }
}

/// A jar directly in `plugins/` and its declaration, if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarInPlugins {
    pub file_name: String,
    pub declaration: Option<Declaration>,
}

/// `plugins/` of the server directory.
pub fn path(server: &ServerDirectory) -> PathBuf {
    server.path.join(PLUGINS_DIRECTORY_NAME)
}

/// The jars placed in `plugins/` and their records in `plugins` of `mism.toml` (plg-4).
pub fn placed_jars(server: &ServerDirectory) -> PlacedFiles<'_> {
    PlacedFiles {
        server,
        directory: path(server),
        suffix: JAR_SUFFIX,
        records: &server.mism_toml.plugins,
        with_records: |mism_toml, plugins| MismToml {
            plugins,
            ..mism_toml.clone()
        },
    }
}

/// Each jar directly in `plugins/` by file name, with its declaration; none without `plugins/`
/// (insp-3).
pub fn jars(server: &ServerDirectory) -> Result<Vec<JarInPlugins>, Error> {
    let plugins = path(server);
    placed_files::file_names(&plugins, JAR_SUFFIX)?
        .into_iter()
        .map(|file_name| {
            let path = plugins.join(&file_name);
            let declaration = plugin_jar::read_declaration(&path)
                .map_err(|source| Error::UnreadableDeclaration { path, source })?;
            Ok(JarInPlugins {
                file_name,
                declaration,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Cursor, Write};
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    /// A zip archive with each entry's path and content.
    fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    /// A server directory in a temporary directory.
    struct Fixture {
        server: ServerDirectory,
        _root: TempDir,
    }

    impl Fixture {
        fn new() -> Fixture {
            let root = temporary_directory();
            Fixture {
                server: ServerDirectory {
                    path: root.path().to_path_buf(),
                    mism_toml: MismToml {
                        software: "paper".to_owned(),
                        minecraft_version: "26.3".to_owned(),
                        software_version: 206,
                        plugins: Vec::new(),
                        datapacks: Vec::new(),
                    },
                },
                _root: root,
            }
        }

        fn plugins(&self) -> PathBuf {
            self.server.path.join("plugins")
        }

        fn put_in_plugins(&self, file_name: &str, content: &[u8]) {
            fs::create_dir_all(self.plugins()).unwrap();
            fs::write(self.plugins().join(file_name), content).unwrap();
        }
    }

    /// 「プラグインとデータパックを置くディレクトリ」 方法: the jars in `plugins/` with their
    /// declarations; a jar without one has none.
    #[test]
    fn jars_in_plugins_have_their_declarations() {
        let fixture = Fixture::new();
        fixture.put_in_plugins("Vault.jar", &jar(&[("plugin.yml", b"name: Vault\n")]));
        fixture.put_in_plugins("Nameless.jar", &jar(&[("README", b"")]));

        let names: Vec<_> = jars(&fixture.server)
            .unwrap()
            .into_iter()
            .map(|jar| {
                (
                    jar.file_name,
                    jar.declaration.and_then(|declaration| declaration.name),
                )
            })
            .collect();

        assert_eq!(
            names,
            [
                ("Nameless.jar".to_owned(), None),
                ("Vault.jar".to_owned(), Some("Vault".to_owned()))
            ]
        );
    }

    /// 「プラグインとデータパックを置くディレクトリ」 方法: a jar whose declaration cannot be read is
    /// an error whose hint is to move it out.
    #[test]
    fn jar_with_an_unreadable_declaration_is_an_error() {
        let fixture = Fixture::new();
        fixture.put_in_plugins("Broken.jar", b"not a zip file");

        let error = jars(&fixture.server).unwrap_err();

        assert_eq!(
            error.hint(),
            Some(format!(
                "move {} out of plugins/",
                fixture.plugins().join("Broken.jar").display()
            ))
        );
    }

    /// 「プラグインとデータパックを置くディレクトリ」 方法: without `plugins/` there are no jars.
    #[test]
    fn server_without_plugins_has_no_jars() {
        let fixture = Fixture::new();

        assert_eq!(jars(&fixture.server).unwrap(), []);
    }
}
