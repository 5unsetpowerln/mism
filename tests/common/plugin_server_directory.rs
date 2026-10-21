use std::ffi::OsString;
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;

use super::management_environment::{ManagementEnvironment, entries_below, mism_toml_with_records};
use super::mism_process::{MismProcess, Output};

pub use super::management_environment::Record;

/// The text of a `mism.toml` with the three keys `mism new` writes and the plugin records.
pub fn mism_toml_text(software: &str, minecraft_version: &str, records: &[Record]) -> String {
    mism_toml_with_records(software, minecraft_version, records, &[])
}

/// What plg-8 keeps as it was: the text of `mism.toml` and everything below `plugins/`, each
/// file with its content and each directory without.
#[derive(Debug, PartialEq, Eq)]
pub struct ServerState {
    mism_toml: Option<String>,
    plugins: Vec<(PathBuf, Option<Vec<u8>>)>,
}

/// Everything one `mism plugin` run needs around it (see `ManagementEnvironment`).
pub struct PluginEnvironment(ManagementEnvironment);

impl Deref for PluginEnvironment {
    type Target = ManagementEnvironment;

    fn deref(&self) -> &ManagementEnvironment {
        &self.0
    }
}

impl PluginEnvironment {
    /// An environment whose `server` has no `mism.toml`, so there is no target server.
    pub fn without_server() -> PluginEnvironment {
        PluginEnvironment(ManagementEnvironment::new())
    }

    /// An environment whose `server` is a server directory of `software` (`paper` or `folia`)
    /// for `minecraft_version`, without plugin records. The tests do not run `mism new`.
    pub fn with_server(software: &str, minecraft_version: &str) -> PluginEnvironment {
        let environment = PluginEnvironment::without_server();
        environment.write_mism_toml(&mism_toml_text(software, minecraft_version, &[]));
        environment
    }

    pub fn plugins(&self) -> PathBuf {
        self.server().join("plugins")
    }

    /// Writes `bytes` as `plugins/<file_name>`, creating `plugins/` if needed.
    pub fn put_in_plugins(&self, file_name: &str, bytes: &[u8]) -> PathBuf {
        fs::create_dir_all(self.plugins()).unwrap();
        let path = self.plugins().join(file_name);
        fs::write(&path, bytes).unwrap();
        path
    }

    /// Writes `bytes` as the local jar file `outside/<file_name>` and returns its path.
    pub fn write_local_jar(&self, file_name: &str, bytes: &[u8]) -> PathBuf {
        self.write_outside(file_name, bytes)
    }

    /// `mism plugin fetch <source> --yes`, run to its exit: `--yes` keeps a URL fetch from asking
    /// for the name.
    pub fn fetch(&self, source: &str) -> Output {
        self.mism_plugin(["fetch", source, "--yes"]).run()
    }

    pub fn server_state(&self) -> ServerState {
        ServerState {
            mism_toml: fs::read_to_string(self.server().join("mism.toml")).ok(),
            plugins: entries_below(&self.plugins()),
        }
    }

    /// `mism plugin <arguments>` with `HOME`, `XDG_CACHE_HOME`, and `MISM_MODRINTH_URL` set, run
    /// in `server`.
    pub fn mism_plugin<I, S>(&self, arguments: I) -> MismProcess
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.mism("plugin", &self.server(), arguments)
    }
}
