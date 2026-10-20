use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{self, Path, PathBuf};

use sha2::{Digest, Sha256};

use super::ServerDirectory;
use super::initial_launch;
use super::mism_toml::{self, MismToml};
use crate::atomic_fs;
use crate::interruption::{self, Signal};

const WORKING_DIRECTORY_PREFIX: &str = ".mism-tmp-";
const RANDOM_SOURCE: &str = "/dev/urandom";
const MISM_TOML_FILE_NAME: &str = "mism.toml";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to make {} an absolute path", path.display())]
    MakeAbsolute {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} already exists", path.display())]
    AlreadyExists { path: PathBuf },
    #[error("failed to check whether {} exists", path.display())]
    CheckPath {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the parent directory {} does not exist", path.display())]
    ParentMissing { path: PathBuf },
    #[error("the parent {} is not a directory", path.display())]
    ParentNotDirectory { path: PathBuf },
    #[error("failed to check the parent directory {}", path.display())]
    CheckParent {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read random bytes from {RANDOM_SOURCE}")]
    ReadRandom(#[source] io::Error),
    #[error("failed to create the working directory {}", path.display())]
    CreateWorkingDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to copy {} to {}", from.display(), to.display())]
    CopyJar {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read the copied jar {}", path.display())]
    ReadCopiedJar {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the SHA-256 of the copied jar {} is {actual}, but PaperMC publishes {expected}", path.display())]
    CopiedJarSha256Mismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error(transparent)]
    InitialLaunch(#[from] initial_launch::Error),
    #[error(transparent)]
    MismToml(#[from] mism_toml::Error),
    #[error("interrupted by {0}")]
    Interrupted(Signal),
    #[error("{} appeared while the server directory was being prepared", path.display())]
    TargetAppeared { path: PathBuf },
    #[error(transparent)]
    Rename(atomic_fs::Error),
    #[error("failed to remove the working directory {} ({removal}) after this failure", path.display())]
    LeftWorkingDirectory {
        path: PathBuf,
        removal: io::Error,
        #[source]
        failure: Box<Error>,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::InitialLaunch(error) => error.hint(),
            Error::MismToml(error) => error.hint(),
            Error::Rename(error) => error.hint(),
            Error::LeftWorkingDirectory { failure, .. } => failure.hint(),
            _ => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::InitialLaunch(error) => error.interrupted_by(),
            Error::Interrupted(signal) => Some(*signal),
            Error::LeftWorkingDirectory { failure, .. } => failure.interrupted_by(),
            _ => None,
        }
    }
}

/// The absolute form of the path the user gave, at which nothing exists and whose parent is a
/// directory (steps 1 to 3).
#[derive(Debug)]
pub struct TargetPath {
    path: PathBuf,
    parent: PathBuf,
}

/// The verified jar in the cache directory, with the name and SHA-256 PaperMC publishes for it.
pub struct CachedJar<'a> {
    pub path: &'a Path,
    pub name: &'a str,
    pub sha256: &'a str,
}

/// Steps 1 to 3: makes `path` absolute and checks that nothing exists at it and that its parent
/// is a directory.
pub fn check_target_path(path: &Path) -> Result<TargetPath, Error> {
    let path = make_absolute(path)?;
    check_nothing_exists(&path)?;
    let parent = path
        .parent()
        .expect("nothing exists at the absolute path, so it is not `/` and has a parent")
        .to_path_buf();
    check_parent_is_directory(&parent)?;
    Ok(TargetPath { path, parent })
}

/// Steps 5 to 8: prepares the server in a working directory beside `target` and renames that
/// directory to `target`. On a failure after step 5 the working directory is removed (err-8).
pub fn create_server_directory(
    target: TargetPath,
    jar: &CachedJar,
    mism_toml: MismToml,
    minimum_java_version: u32,
) -> Result<ServerDirectory, Error> {
    let working_directory = create_working_directory(&target.parent)?;
    match prepare_and_rename(
        &working_directory,
        &target.path,
        jar,
        &mism_toml,
        minimum_java_version,
    ) {
        Ok(()) => Ok(ServerDirectory {
            path: target.path,
            mism_toml,
        }),
        Err(failure) => Err(remove_working_directory(&working_directory, failure)),
    }
}

/// Step 1 (new-1).
fn make_absolute(path: &Path) -> Result<PathBuf, Error> {
    path::absolute(path).map_err(|source| Error::MakeAbsolute {
        path: path.to_path_buf(),
        source,
    })
}

/// Step 2 (err-1): `lstat`, so that a symbolic link counts even when its target does not exist.
fn check_nothing_exists(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(Error::AlreadyExists {
            path: path.to_path_buf(),
        }),
        // ENOTDIR: a component of the parent is not a directory, which step 3 reports.
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(())
        }
        Err(source) => Err(Error::CheckPath {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Step 3 (err-2).
fn check_parent_is_directory(parent: &Path) -> Result<(), Error> {
    let path = parent.to_path_buf();
    match fs::metadata(parent) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(Error::ParentNotDirectory { path }),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Err(Error::ParentMissing { path })
        }
        Err(source) => Err(Error::CheckParent { path, source }),
    }
}

/// Step 5.
fn create_working_directory(parent: &Path) -> Result<PathBuf, Error> {
    let path = parent.join(format!(
        "{WORKING_DIRECTORY_PREFIX}{}",
        random_hex_digits()?
    ));
    fs::create_dir(&path).map_err(|source| Error::CreateWorkingDirectory {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// 16 lowercase hexadecimal digits from 8 random bytes.
fn random_hex_digits() -> Result<String, Error> {
    let mut bytes = [0; 8];
    File::open(RANDOM_SOURCE)
        .and_then(|mut random| random.read_exact(&mut bytes))
        .map_err(Error::ReadRandom)?;
    Ok(to_hex(&bytes))
}

/// Steps 6 to 8.
fn prepare_and_rename(
    working_directory: &Path,
    target: &Path,
    jar: &CachedJar,
    mism_toml: &MismToml,
    minimum_java_version: u32,
) -> Result<(), Error> {
    copy_jar(working_directory, jar)?;
    initial_launch::launch(working_directory, jar.name, minimum_java_version)?;
    write_mism_toml(working_directory, mism_toml)?;
    fail_if_interrupted()?;
    rename_to_target(working_directory, target)
}

/// Step 6.
fn copy_jar(working_directory: &Path, jar: &CachedJar) -> Result<(), Error> {
    let copy = working_directory.join(jar.name);
    fs::copy(jar.path, &copy).map_err(|source| Error::CopyJar {
        from: jar.path.to_path_buf(),
        to: copy.clone(),
        source,
    })?;
    verify_sha256(&copy, jar.sha256)
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), Error> {
    let actual = sha256_of_file(path).map_err(|source| Error::ReadCopiedJar {
        path: path.to_path_buf(),
        source,
    })?;
    if actual != expected {
        return Err(Error::CopiedJarSha256Mismatch {
            path: path.to_path_buf(),
            expected: expected.to_owned(),
            actual,
        });
    }
    Ok(())
}

fn sha256_of_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0; 64 * 1024];
    loop {
        match file.read(&mut chunk)? {
            0 => return Ok(to_hex(&hasher.finalize())),
            length => hasher.update(&chunk[..length]),
        }
    }
}

/// Step 7 of 「初回起動」 (new-9).
fn write_mism_toml(working_directory: &Path, mism_toml: &MismToml) -> Result<(), Error> {
    Ok(mism_toml.write(&working_directory.join(MISM_TOML_FILE_NAME))?)
}

/// Notices a signal that arrived after the server exited, at the last moment at which nothing is
/// at the target path yet (int-1).
fn fail_if_interrupted() -> Result<(), Error> {
    match interruption::received() {
        Some(signal) => Err(Error::Interrupted(signal)),
        None => Ok(()),
    }
}

/// Step 8.
fn rename_to_target(working_directory: &Path, target: &Path) -> Result<(), Error> {
    atomic_fs::rename_without_replace(working_directory, target).map_err(|error| match error {
        // d-new-2
        atomic_fs::Error::AlreadyExists { path } => Error::TargetAppeared { path },
        error => Error::Rename(error),
    })
}

/// Removes the working directory after `failure`, and returns the error to report.
fn remove_working_directory(working_directory: &Path, failure: Error) -> Error {
    match fs::remove_dir_all(working_directory) {
        Ok(()) => failure,
        Err(removal) => Error::LeftWorkingDirectory {
            path: working_directory.to_path_buf(),
            removal,
            failure: Box::new(failure),
        },
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;

    const JAR_NAME: &str = "paper-26.3-206.jar";
    const JAR_BYTES: &[u8] = b"pretend this is a jar";

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        to_hex(&Sha256::digest(bytes))
    }

    fn entry_names(directory: &Path) -> Vec<String> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect()
    }

    /// Writes `JAR_BYTES` as the cached jar in `cache` and returns its path.
    fn put_cached_jar(cache: &Path) -> PathBuf {
        let path = cache.join(JAR_NAME);
        fs::write(&path, JAR_BYTES).unwrap();
        path
    }

    fn paper_26_3_206() -> MismToml {
        MismToml {
            software: "paper".to_owned(),
            minecraft_version: "26.3".to_owned(),
            software_version: 206,
        }
    }

    /// 「mism new の手順と中断」 方法 1 (new-1): a relative path is joined to the current directory.
    #[test]
    fn relative_path_is_joined_to_the_current_directory() {
        assert_eq!(
            make_absolute(Path::new("servers/survival")).unwrap(),
            env::current_dir().unwrap().join("servers/survival")
        );
    }

    /// 「mism new の手順と中断」 方法 1: `.` components are dropped from the absolute path.
    #[test]
    fn dot_components_are_dropped_from_the_absolute_path() {
        assert_eq!(
            make_absolute(Path::new("./survival/.")).unwrap(),
            env::current_dir().unwrap().join("survival")
        );
    }

    /// 「mism new の手順と中断」 方法 1: an empty path has no absolute form.
    #[test]
    fn empty_path_cannot_be_made_absolute() {
        assert!(matches!(
            make_absolute(Path::new("")),
            Err(Error::MakeAbsolute { .. })
        ));
    }

    /// 「mism new の手順と中断」 方法 2 and 3 (err-2): a path below a file has a parent that is not
    /// a directory.
    #[test]
    fn path_below_a_file_has_a_parent_that_is_not_a_directory() {
        let directory = temporary_directory();
        let file = directory.path().join("file");
        fs::write(&file, "").unwrap();

        let result = check_target_path(&file.join("survival"));

        assert!(matches!(result, Err(Error::ParentNotDirectory { path }) if path == file));
    }

    /// 「mism new の手順と中断」 方法 3: a symbolic link to a directory is a parent directory.
    #[test]
    fn parent_that_links_to_a_directory_is_accepted() {
        let directory = temporary_directory();
        let link = directory.path().join("link");
        symlink(directory.path(), &link).unwrap();

        let result = check_target_path(&link.join("survival"));

        assert!(result.is_ok(), "{result:?}");
    }

    /// 「mism new の手順と中断」 方法 5: the working directory is named `.mism-tmp-` and 16
    /// lowercase hexadecimal digits.
    #[test]
    fn working_directory_is_named_with_16_hexadecimal_digits() {
        let parent = temporary_directory();

        let working_directory = create_working_directory(parent.path()).unwrap();

        let name = working_directory.file_name().unwrap().to_str().unwrap();
        let digits = name.strip_prefix(".mism-tmp-").unwrap_or_default();
        assert!(
            digits.len() == 16
                && digits
                    .bytes()
                    .all(|byte| b"0123456789abcdef".contains(&byte)),
            "{name}"
        );
    }

    /// 「mism new の手順と中断」 方法 5: the working directory is created in the parent.
    #[test]
    fn working_directory_is_created_in_the_parent() {
        let parent = temporary_directory();

        let working_directory = create_working_directory(parent.path()).unwrap();

        assert_eq!(
            entry_names(parent.path()),
            [working_directory.file_name().unwrap().to_str().unwrap()]
        );
    }

    /// 「mism new の手順と中断」 方法 6: the copy has the cached jar's content and the distributed
    /// name.
    #[test]
    fn copied_jar_has_the_cached_content() {
        let cache = temporary_directory();
        let working_directory = temporary_directory();
        let cached_path = put_cached_jar(cache.path());
        let sha256 = sha256_hex(JAR_BYTES);

        copy_jar(
            working_directory.path(),
            &CachedJar {
                path: &cached_path,
                name: JAR_NAME,
                sha256: &sha256,
            },
        )
        .unwrap();

        assert_eq!(
            fs::read(working_directory.path().join(JAR_NAME)).unwrap(),
            JAR_BYTES
        );
    }

    /// 「mism new の手順と中断」 方法 6: a copy whose SHA-256 is not the published one is an error.
    #[test]
    fn copied_jar_with_another_sha256_is_an_error() {
        let cache = temporary_directory();
        let working_directory = temporary_directory();
        let cached_path = put_cached_jar(cache.path());
        let sha256 = sha256_hex(b"another jar");

        let result = copy_jar(
            working_directory.path(),
            &CachedJar {
                path: &cached_path,
                name: JAR_NAME,
                sha256: &sha256,
            },
        );

        assert!(matches!(result, Err(Error::CopiedJarSha256Mismatch { .. })));
    }

    /// 「mism new の手順と中断」 方法 8 (d-new-2): something that appeared at the path after the
    /// checks makes the rename fail.
    #[test]
    fn rename_onto_a_path_that_appeared_is_an_error() {
        let parent = temporary_directory();
        let working_directory = parent.path().join(".mism-tmp-0123456789abcdef");
        fs::create_dir(&working_directory).unwrap();
        let target = parent.path().join("survival");
        fs::create_dir(&target).unwrap();

        let result = rename_to_target(&working_directory, &target);

        assert!(matches!(result, Err(Error::TargetAppeared { path }) if path == target));
    }

    /// 「mism new の手順と中断」 方法 (err-8): a failure after step 5 removes the working
    /// directory.
    #[test]
    fn failure_after_creating_the_working_directory_removes_it() {
        let parent = temporary_directory();
        let cache = temporary_directory();
        let cached_path = put_cached_jar(cache.path());
        let target = check_target_path(&parent.path().join("survival")).unwrap();
        let sha256 = sha256_hex(b"another jar");

        let _ = create_server_directory(
            target,
            &CachedJar {
                path: &cached_path,
                name: JAR_NAME,
                sha256: &sha256,
            },
            paper_26_3_206(),
            25,
        );

        assert_eq!(entry_names(parent.path()), Vec::<String>::new());
    }

    /// 「mism new の手順と中断」 方法 (err-8): a working directory that cannot be removed is
    /// reported together with the failure that made mism remove it.
    #[test]
    fn working_directory_that_cannot_be_removed_is_reported_with_the_failure() {
        let parent = temporary_directory();
        let target = parent.path().join("survival");

        let error = remove_working_directory(
            &parent.path().join(".mism-tmp-0123456789abcdef"),
            Error::TargetAppeared {
                path: target.clone(),
            },
        );

        assert!(matches!(
            error,
            Error::LeftWorkingDirectory { failure, .. }
                if matches!(*failure, Error::TargetAppeared { ref path } if *path == target)
        ));
    }
}
