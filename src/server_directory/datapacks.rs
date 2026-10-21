use std::path::PathBuf;

use super::ServerDirectory;
use super::mism_toml::MismToml;
use super::placed_files::PlacedFiles;
use super::server_properties;

const DATAPACKS_DIRECTORY_NAME: &str = "datapacks";
const ZIP_SUFFIX: &str = ".zip";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    ServerProperties(#[from] server_properties::Error),
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::ServerProperties(error) => error.hint(),
        }
    }
}

/// `datapacks/` of the world directory that `level-name` names: relative to the server directory,
/// or an absolute path as it is (dp-3).
pub fn path(server: &ServerDirectory) -> Result<PathBuf, Error> {
    let level_name = server_properties::read_level_name(&server.path)?;
    // Joining an absolute path gives that path itself; nothing is normalized (dp-3).
    Ok(server.path.join(level_name).join(DATAPACKS_DIRECTORY_NAME))
}

/// The datapack files placed in `directory`, the world's `datapacks/`, and their records in
/// `datapacks` of `mism.toml` (dp-7).
pub fn placed_datapacks(server: &ServerDirectory, directory: PathBuf) -> PlacedFiles<'_> {
    PlacedFiles {
        server,
        directory,
        suffix: ZIP_SUFFIX,
        records: &server.mism_toml.datapacks,
        with_records: |mism_toml, datapacks| MismToml {
            datapacks,
            ..mism_toml.clone()
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::server_directory::mism_toml::Record;
    use crate::server_directory::placed_files::{FileToPlace, Removed, State};

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn record(name: &str, file_name: &str, sha256: &str) -> Record {
        Record {
            name: name.to_owned(),
            source: format!("https://example.com/{file_name}"),
            file_name: file_name.to_owned(),
            sha256: sha256.to_owned(),
        }
    }

    /// A server directory with its `mism.toml` and `server.properties` written, a cached file
    /// outside it, and the temporary directories holding both.
    struct Fixture {
        server: ServerDirectory,
        _server_root: TempDir,
        cache: TempDir,
    }

    impl Fixture {
        fn with_records(plugins: Vec<Record>, datapacks: Vec<Record>, level_name: &str) -> Fixture {
            let server_root = temporary_directory();
            let server = ServerDirectory {
                path: server_root.path().to_path_buf(),
                mism_toml: MismToml {
                    software: "paper".to_owned(),
                    minecraft_version: "26.3".to_owned(),
                    software_version: 206,
                    plugins,
                    datapacks,
                },
            };
            server.mism_toml.write(&server.mism_toml_path()).unwrap();
            fs::write(
                server.path.join("server.properties"),
                format!("level-name={level_name}\n"),
            )
            .unwrap();
            Fixture {
                server,
                _server_root: server_root,
                cache: temporary_directory(),
            }
        }

        /// A server whose `server.properties` has `level-name=<level_name>`, without records.
        fn with_level_name(level_name: &str) -> Fixture {
            Fixture::with_records(Vec::new(), Vec::new(), level_name)
        }

        fn datapacks(&self) -> PlacedFiles<'_> {
            placed_datapacks(&self.server, path(&self.server).unwrap())
        }

        fn put_in(&self, directory: &Path, file_name: &str, content: &[u8]) {
            fs::create_dir_all(directory).unwrap();
            fs::write(directory.join(file_name), content).unwrap();
        }

        /// Places a cached datapack file `Pack.zip` with `content` in the world's `datapacks/`.
        fn place_datapack(&self, content: &[u8]) {
            let sha256 = sha256_hex(content);
            let directory = self.cache.path().join(&sha256);
            fs::create_dir(&directory).unwrap();
            let cached_path = directory.join("Pack.zip");
            fs::write(&cached_path, content).unwrap();
            self.datapacks()
                .place(&FileToPlace {
                    cached_path: &cached_path,
                    sha256: &sha256,
                    name: "Pack",
                    source: "https://example.com/Pack.zip",
                    file_name: "Pack.zip",
                })
                .unwrap();
        }

        fn read_mism_toml(&self) -> MismToml {
            MismToml::read(&self.server.mism_toml_path()).unwrap()
        }
    }

    /// dplace-3: only a datapack record with the same name prevents placing a datapack.
    #[test]
    fn plugin_record_with_the_same_name_does_not_prevent_placing_a_datapack() {
        let fixture = Fixture::with_records(
            vec![record("Pack", "Pack.jar", &"1".repeat(64))],
            Vec::new(),
            "world",
        );

        assert!(
            fixture
                .datapacks()
                .check_placeable("Pack", "Pack.zip")
                .is_ok()
        );
    }

    /// dplace-2: the datapack's record is added to `datapacks` of `mism.toml`.
    #[test]
    fn placed_datapack_is_recorded_in_datapacks() {
        let fixture = Fixture::with_level_name("world");

        fixture.place_datapack(b"zip content");

        let mism_toml = fixture.read_mism_toml();
        assert_eq!(
            (mism_toml.plugins, mism_toml.datapacks),
            (
                Vec::new(),
                vec![record("Pack", "Pack.zip", &sha256_hex(b"zip content"))]
            )
        );
    }

    /// dp-3: an absolute `level-name` is the world directory itself.
    #[test]
    fn absolute_level_name_is_the_world_directory() {
        let outside = temporary_directory();
        let fixture = Fixture::with_level_name(outside.path().join("world").to_str().unwrap());

        assert_eq!(
            path(&fixture.server).unwrap(),
            outside.path().join("world/datapacks")
        );
    }

    /// dp-3: `level-name` is joined to the server directory without being normalized.
    #[test]
    fn level_name_is_not_normalized() {
        let fixture = Fixture::with_level_name("../outside-world");

        assert_eq!(
            path(&fixture.server).unwrap(),
            fixture.server.path.join("../outside-world/datapacks")
        );
    }

    /// dp-3: without `server.properties` the datapack directory is not determined.
    #[test]
    fn datapack_directory_needs_server_properties() {
        let fixture = Fixture::with_level_name("world");
        fs::remove_file(fixture.server.path.join("server.properties")).unwrap();

        assert!(matches!(
            path(&fixture.server),
            Err(Error::ServerProperties(_))
        ));
    }

    /// drm-1: a datapack's removal removes its file and its record, and keeps the plugins'.
    #[test]
    fn removal_of_a_datapack_keeps_the_plugin_records() {
        let plugin = record("Pack", "Pack.jar", &"1".repeat(64));
        let datapack = record("Pack", "Pack.zip", &"2".repeat(64));
        let fixture = Fixture::with_records(vec![plugin.clone()], vec![datapack.clone()], "world");
        fixture.put_in(&fixture.datapacks().directory, "Pack.zip", b"zip");

        let removed = fixture.datapacks().remove("Pack").unwrap();

        let mism_toml = fixture.read_mism_toml();
        assert_eq!(
            (
                removed,
                fixture.datapacks().directory.join("Pack.zip").exists(),
                mism_toml.plugins,
                mism_toml.datapacks
            ),
            (
                Removed::FileAndRecord(datapack),
                false,
                vec![plugin],
                Vec::new()
            )
        );
    }

    /// dlist-2: a datapack record whose file is in another world's directory is `missing`.
    #[test]
    fn datapack_record_without_its_file_in_the_world_is_missing() {
        let fixture = Fixture::with_records(
            Vec::new(),
            vec![record("Pack", "Pack.zip", &sha256_hex(b"zip"))],
            "other-world",
        );
        let old_world: PathBuf = fixture.server.path.join("world/datapacks");
        fixture.put_in(&old_world, "Pack.zip", b"zip");

        let states: Vec<_> = fixture
            .datapacks()
            .reconcile()
            .unwrap()
            .into_iter()
            .map(|state| state.state)
            .collect();

        assert_eq!(states, [State::Missing]);
    }
}
