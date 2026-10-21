use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::fake_modrinth::FakeModrinth;
use super::mism_process::MismProcess;
use super::temporary_directory;

/// The record of a plugin or a datapack in `mism.toml` (plg-4, dp-7), as a test writes it.
#[derive(Clone, Copy)]
pub struct Record<'a> {
    pub name: &'a str,
    pub source: &'a str,
    pub file_name: &'a str,
    pub sha256: &'a str,
}

/// The text of a `mism.toml` with the three keys `mism new` writes, the records of plugins, and
/// the records of datapacks.
pub fn mism_toml_with_records(
    software: &str,
    minecraft_version: &str,
    plugins: &[Record],
    datapacks: &[Record],
) -> String {
    let mut text = format!(
        "software = \"{software}\"\nminecraft_version = \"{minecraft_version}\"\nsoftware_version = 1\n"
    );
    for (array, records) in [("plugins", plugins), ("datapacks", datapacks)] {
        for record in records {
            text.push_str(&format!(
                "\n[[{array}]]\nname = {:?}\nsource = {:?}\nfile_name = {:?}\nsha256 = {:?}\n",
                record.name, record.source, record.file_name, record.sha256
            ));
        }
    }
    text
}

/// Every entry below `directory` at any depth, sorted: a file with its content, a directory
/// without. Empty when `directory` does not exist.
pub fn entries_below(directory: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
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

/// The environment that the tests of the plugin management and of the datapack management share:
/// a temporary directory holding the home directory, the cache directory, a directory `server` to
/// run mism in, and a directory `outside` for local files; and a fake Modrinth.
/// `PluginEnvironment` and `DatapackEnvironment` add what the tests of each need.
pub struct ManagementEnvironment {
    root: TempDir,
    pub modrinth: FakeModrinth,
}

impl ManagementEnvironment {
    /// An environment whose `server` is empty.
    pub fn new() -> ManagementEnvironment {
        let root = temporary_directory::create();
        for directory in ["home", "server", "outside"] {
            fs::create_dir(root.path().join(directory)).unwrap();
        }
        ManagementEnvironment {
            root,
            modrinth: FakeModrinth::start(),
        }
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

    /// The directory mism runs in.
    pub fn server(&self) -> PathBuf {
        self.root.path().join("server")
    }

    /// A directory beside `server`, for local files and for whatever a test keeps out of `server`.
    pub fn outside(&self) -> PathBuf {
        self.root.path().join("outside")
    }

    /// Writes `bytes` as the local file `outside/<file_name>` and returns its path.
    pub fn write_outside(&self, file_name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.outside().join(file_name);
        fs::write(&path, bytes).unwrap();
        path
    }

    /// Every file below the cache directory, at any depth.
    pub fn files_in_cache(&self) -> Vec<PathBuf> {
        entries_below(&self.cache())
            .into_iter()
            .filter(|(_, content)| content.is_some())
            .map(|(path, _)| path)
            .collect()
    }

    /// `mism <kind> <arguments>` (`kind` being `plugin` or `datapack`) with `HOME`,
    /// `XDG_CACHE_HOME`, and `MISM_MODRINTH_URL` set, run in `directory`.
    pub fn mism<I, S>(&self, kind: &str, directory: &Path, arguments: I) -> MismProcess
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        MismProcess::new()
            .arguments([kind])
            .arguments(arguments)
            .environment("HOME", self.home())
            .environment("XDG_CACHE_HOME", self.xdg_cache_home())
            .environment("MISM_MODRINTH_URL", self.modrinth.base_url())
            .working_directory(directory)
    }
}
