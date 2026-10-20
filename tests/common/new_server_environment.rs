use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::fake_java::{Behavior, FakeJava};
use super::fake_papermc::{self, FakePaperMc};
use super::mmmm_process::MmmmProcess;
use super::temporary_directory;

/// What the fake PaperMC serves as the jar of Paper 26.3 206.
pub const JAR_BYTES: &[u8] = b"pretend this is paper-26.3-206.jar";

/// Everything one `mmmm new` run needs around it: a temporary directory holding the home
/// directory, a directory to create servers in, and a fake `java`, and a fake PaperMC.
pub struct NewServerEnvironment {
    root: TempDir,
    pub papermc: FakePaperMc,
    pub fake_java: FakeJava,
}

impl NewServerEnvironment {
    /// An environment whose fake PaperMC serves nothing yet.
    pub fn new(behavior: Behavior) -> NewServerEnvironment {
        let root = temporary_directory::create();
        fs::create_dir(root.path().join("home")).unwrap();
        fs::create_dir(root.path().join("servers")).unwrap();
        let fake_java = FakeJava::create(root.path().join("java"), behavior);
        NewServerEnvironment {
            root,
            papermc: FakePaperMc::start(),
            fake_java,
        }
    }

    /// An environment whose fake PaperMC serves the saved Paper version list and `JAR_BYTES` as
    /// Paper 26.3 206 with their true SHA-256.
    pub fn serving_paper_26_3(behavior: Behavior) -> NewServerEnvironment {
        let environment = NewServerEnvironment::new(behavior);
        environment.papermc.mount_version_list_from_fixture("paper");
        environment.papermc.mount_jar_download(
            "paper",
            "26.3",
            206,
            JAR_BYTES,
            &fake_papermc::sha256_hex(JAR_BYTES),
        );
        environment
    }

    pub fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    /// A directory for `XDG_CACHE_HOME`; it does not exist until mmmm or a test creates it.
    pub fn xdg_cache_home(&self) -> PathBuf {
        self.root.path().join("xdg-cache")
    }

    /// The existing directory the tests create server directories in.
    pub fn servers(&self) -> PathBuf {
        self.root.path().join("servers")
    }

    /// `<servers>/survival`, where nothing exists before mmmm runs.
    pub fn server_path(&self) -> PathBuf {
        self.servers().join("survival")
    }

    /// `mmmm new <arguments>` with `HOME`, `PATH` (the fake `java` first, then the directories of
    /// `sh`, `mv`, and `sleep`), and `MMMM_PAPERMC_URL` set, run from the temporary directory.
    pub fn mmmm_new<I, S>(&self, arguments: I) -> MmmmProcess
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut path = OsString::from(self.fake_java.directory());
        path.push(":/usr/bin:/bin");
        MmmmProcess::new()
            .arguments(["new"])
            .arguments(arguments)
            .environment("HOME", self.home())
            .environment("PATH", path)
            .environment("MMMM_PAPERMC_URL", self.papermc.base_url())
            .working_directory(self.root.path())
    }
}
