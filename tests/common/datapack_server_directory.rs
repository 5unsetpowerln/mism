use std::ffi::OsString;
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;

use super::management_environment::{ManagementEnvironment, entries_below, mism_toml_with_records};
use super::mism_process::{MismProcess, Output};
use super::test_datapack_zip::{pack_version, server_jar, version_json};

/// The text of a `server.properties` as Paper writes it, with `level-name` set to `level_name`.
fn server_properties_text(level_name: &str) -> String {
    format!(
        "#Minecraft server properties\n#Wed Oct 21 10:00:00 JST 2026\nlevel-name={level_name}\n\
         level-type=minecraft\\:normal\nmotd=A Minecraft Server\n"
    )
}

/// What dp-11 keeps as it was: the text of `mism.toml`, whether anything is at the path of the
/// world directory, and everything below the world directory, each file with its content and
/// each directory without.
#[derive(Debug, PartialEq, Eq)]
pub struct WorldState {
    mism_toml: Option<String>,
    world_exists: bool,
    world: Vec<(PathBuf, Option<Vec<u8>>)>,
}

/// Everything one `mism datapack` run needs around it (see `ManagementEnvironment`), with the
/// `level-name` that the environment wrote in `server/server.properties`.
pub struct DatapackEnvironment {
    base: ManagementEnvironment,
    level_name: String,
}

impl Deref for DatapackEnvironment {
    type Target = ManagementEnvironment;

    fn deref(&self) -> &ManagementEnvironment {
        &self.base
    }
}

impl DatapackEnvironment {
    /// An environment whose `server` has `server.properties` with `level-name=world` but no
    /// `mism.toml`, so there is no target server.
    pub fn without_server() -> DatapackEnvironment {
        DatapackEnvironment {
            base: ManagementEnvironment::new(),
            level_name: String::new(),
        }
        .with_level_name("world")
    }

    /// An environment whose `server` is a server directory of `software` (`paper` or `folia`)
    /// for `minecraft_version` without records, with `level-name=world` and with the server jar
    /// below `versions/`, whose datapack format is 121.0 as Paper 26.3's. The tests do not run
    /// `mism new`.
    pub fn with_server(software: &str, minecraft_version: &str) -> DatapackEnvironment {
        let environment = DatapackEnvironment::with_server_without_jar(software, minecraft_version);
        environment.put_server_jar(
            software,
            minecraft_version,
            &server_jar(&version_json(minecraft_version, &pack_version(121, 0))),
        );
        environment
    }

    /// Like `with_server`, without the server jar below `versions/`.
    pub fn with_server_without_jar(software: &str, minecraft_version: &str) -> DatapackEnvironment {
        let environment = DatapackEnvironment::without_server();
        environment.write_mism_toml(&mism_toml_with_records(
            software,
            minecraft_version,
            &[],
            &[],
        ));
        environment
    }

    /// The environment with `server.properties` rewritten to have `level-name=<level_name>`.
    pub fn with_level_name(mut self, level_name: &str) -> DatapackEnvironment {
        self.write_server_properties(server_properties_text(level_name).as_bytes());
        self.level_name = level_name.to_owned();
        self
    }

    /// Replaces `server.properties` with `bytes`. `world` stays the directory of the last
    /// `level-name` given to `with_level_name`.
    pub fn write_server_properties(&self, bytes: &[u8]) {
        fs::write(self.server().join("server.properties"), bytes).unwrap();
    }

    pub fn remove_server_properties(&self) {
        fs::remove_file(self.server().join("server.properties")).unwrap();
    }

    /// `versions/<minecraft_version>/<software>-<minecraft_version>.jar` in `server`.
    pub fn server_jar(&self, software: &str, minecraft_version: &str) -> PathBuf {
        self.server()
            .join("versions")
            .join(minecraft_version)
            .join(format!("{software}-{minecraft_version}.jar"))
    }

    /// Writes `bytes` as the server jar of `software` for `minecraft_version`.
    pub fn put_server_jar(&self, software: &str, minecraft_version: &str, bytes: &[u8]) {
        let path = self.server_jar(software, minecraft_version);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// The world directory: the `level-name` relative to `server`, or itself if absolute (dp-3).
    pub fn world(&self) -> PathBuf {
        self.server().join(&self.level_name)
    }

    pub fn datapacks(&self) -> PathBuf {
        self.world().join("datapacks")
    }

    /// Writes `bytes` as `<world>/datapacks/<file_name>`, creating the directories if needed.
    pub fn put_in_datapacks(&self, file_name: &str, bytes: &[u8]) -> PathBuf {
        fs::create_dir_all(self.datapacks()).unwrap();
        let path = self.datapacks().join(file_name);
        fs::write(&path, bytes).unwrap();
        path
    }

    /// Writes `bytes` as the local datapack file `outside/<file_name>` and returns its path.
    pub fn write_local_zip(&self, file_name: &str, bytes: &[u8]) -> PathBuf {
        self.write_outside(file_name, bytes)
    }

    /// `mism datapack fetch <source> --yes`, run to its exit: `--yes` keeps a URL fetch from
    /// asking for the name.
    pub fn fetch(&self, source: &str) -> Output {
        self.mism_datapack(["fetch", source, "--yes"]).run()
    }

    pub fn world_state(&self) -> WorldState {
        WorldState {
            mism_toml: fs::read_to_string(self.server().join("mism.toml")).ok(),
            world_exists: fs::symlink_metadata(self.world()).is_ok(),
            world: entries_below(&self.world()),
        }
    }

    /// `mism datapack <arguments>` with `HOME`, `XDG_CACHE_HOME`, and `MISM_MODRINTH_URL` set,
    /// run in `server`.
    pub fn mism_datapack<I, S>(&self, arguments: I) -> MismProcess
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.mism("datapack", &self.server(), arguments)
    }
}
