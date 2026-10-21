use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::fake_modrinth::FakeModrinth;
use super::mism_process::{MismProcess, Output};
use super::temporary_directory;

/// The record of a plugin in `mism.toml` (plg-4), as a test writes it.
pub struct Record<'a> {
    pub name: &'a str,
    pub source: &'a str,
    pub file_name: &'a str,
    pub sha256: &'a str,
}

/// The text of a `mism.toml` with the three keys `mism new` writes and the records.
pub fn mism_toml_text(software: &str, minecraft_version: &str, records: &[Record]) -> String {
    let mut text = format!(
        "software = \"{software}\"\nminecraft_version = \"{minecraft_version}\"\nsoftware_version = 1\n"
    );
    for record in records {
        text.push_str(&format!(
            "\n[[plugins]]\nname = {:?}\nsource = {:?}\nfile_name = {:?}\nsha256 = {:?}\n",
            record.name, record.source, record.file_name, record.sha256
        ));
    }
    text
}

/// What plg-8 keeps as it was: the text of `mism.toml` and everything below `plugins/`, each
/// file with its content and each directory without.
#[derive(Debug, PartialEq, Eq)]
pub struct ServerState {
    mism_toml: Option<String>,
    plugins: Vec<(PathBuf, Option<Vec<u8>>)>,
}

/// Every entry below `directory` at any depth, sorted: a file with its content, a directory
/// without. Empty when `directory` does not exist.
fn entries_below(directory: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let Ok(listing) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for entry in listing {
        let path = entry.unwrap().path();
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            entries.extend(entries_below(&path));
            entries.push((path, None));
        } else {
            let content = fs::read(&path).ok();
            entries.push((path, content));
        }
    }
    entries.sort();
    entries
}

/// Everything one `mism plugin` run needs around it: a temporary directory holding the home
/// directory, the cache directory, a directory `server` to run mism in, and a directory
/// `outside` for local jar files; and a fake Modrinth.
pub struct PluginEnvironment {
    root: TempDir,
    pub modrinth: FakeModrinth,
}

impl PluginEnvironment {
    /// An environment whose `server` has no `mism.toml`, so there is no target server.
    pub fn without_server() -> PluginEnvironment {
        let root = temporary_directory::create();
        for directory in ["home", "server", "outside"] {
            fs::create_dir(root.path().join(directory)).unwrap();
        }
        PluginEnvironment {
            root,
            modrinth: FakeModrinth::start(),
        }
    }

    /// An environment whose `server` is a server directory of `software` (`paper` or `folia`)
    /// for `minecraft_version`, without plugin records. The tests do not run `mism new`.
    pub fn with_server(software: &str, minecraft_version: &str) -> PluginEnvironment {
        let environment = PluginEnvironment::without_server();
        environment.write_mism_toml(&mism_toml_text(software, minecraft_version, &[]));
        environment
    }

    pub fn write_mism_toml(&self, text: &str) {
        fs::write(self.server().join("mism.toml"), text).unwrap();
    }

    pub fn read_mism_toml(&self) -> String {
        fs::read_to_string(self.server().join("mism.toml")).unwrap()
    }

    pub fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    /// The value of `XDG_CACHE_HOME`; it does not exist until mism creates it.
    pub fn xdg_cache_home(&self) -> PathBuf {
        self.root.path().join("xdg-cache")
    }

    /// The cache directory, `$XDG_CACHE_HOME/mism` (cache-1).
    pub fn cache(&self) -> PathBuf {
        self.xdg_cache_home().join("mism")
    }

    /// The directory mism runs in, which `with_server` made a server directory.
    pub fn server(&self) -> PathBuf {
        self.root.path().join("server")
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
        let path = self.root.path().join("outside").join(file_name);
        fs::write(&path, bytes).unwrap();
        path
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

    /// Every file below the cache directory, at any depth.
    pub fn files_in_cache(&self) -> Vec<PathBuf> {
        entries_below(&self.cache())
            .into_iter()
            .filter(|(_, content)| content.is_some())
            .map(|(path, _)| path)
            .collect()
    }

    /// `mism plugin <arguments>` with `HOME`, `XDG_CACHE_HOME`, and `MISM_MODRINTH_URL` set, run
    /// in `server`.
    pub fn mism_plugin<I, S>(&self, arguments: I) -> MismProcess
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.mism_plugin_in(&self.server(), arguments)
    }

    /// Like `mism_plugin`, run in `directory`.
    pub fn mism_plugin_in<I, S>(&self, directory: &Path, arguments: I) -> MismProcess
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        MismProcess::new()
            .arguments(["plugin"])
            .arguments(arguments)
            .environment("HOME", self.home())
            .environment("XDG_CACHE_HOME", self.xdg_cache_home())
            .environment("MISM_MODRINTH_URL", self.modrinth.base_url())
            .working_directory(directory)
    }
}
