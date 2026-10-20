use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use nix::errno::Errno;
use nix::fcntl::{AT_FDCWD, AtFlags, OFlag, RenameFlags};
use nix::sys::stat::Mode;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the filesystem of {} does not support unnamed files (O_TMPFILE)", directory.display())]
    Unsupported {
        directory: PathBuf,
        #[source]
        source: Errno,
    },
    #[error("failed to create an unnamed file in {}", directory.display())]
    Create {
        directory: PathBuf,
        #[source]
        source: Errno,
    },
    #[error("{} already exists", path.display())]
    AlreadyExists { path: PathBuf },
    #[error("failed to link the written file as {}", path.display())]
    Link {
        path: PathBuf,
        #[source]
        source: Errno,
    },
    #[error("failed to rename {} to {}", from.display(), to.display())]
    Rename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: Errno,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        None
    }
}

/// A file that has no name in its directory until `link_as` gives it one, so no other process
/// sees it half-written and the kernel frees it however the process ends before that.
#[derive(Debug)]
pub struct UnnamedFile {
    file: File,
}

impl UnnamedFile {
    pub fn create_in(directory: &Path) -> Result<UnnamedFile, Error> {
        let flags = OFlag::O_TMPFILE | OFlag::O_WRONLY | OFlag::O_CLOEXEC;
        let descriptor = nix::fcntl::open(directory, flags, Mode::from_bits_truncate(0o644))
            .map_err(|source| {
                let directory = directory.to_path_buf();
                match source {
                    // A kernel without O_TMPFILE sees only its O_DIRECTORY bit and reports EISDIR.
                    Errno::EOPNOTSUPP | Errno::EISDIR => Error::Unsupported { directory, source },
                    _ => Error::Create { directory, source },
                }
            })?;
        Ok(UnnamedFile {
            file: File::from(descriptor),
        })
    }

    /// Fails with `Error::AlreadyExists` when something already has the name `path`.
    pub fn link_as(self, path: &Path) -> Result<(), Error> {
        // `File` keeps no userspace buffer, so everything written is already in the kernel here.
        // Linking the /proc magic link needs no capability, unlike linkat with AT_EMPTY_PATH.
        let descriptor_path = format!("/proc/self/fd/{}", self.file.as_raw_fd());
        nix::unistd::linkat(
            AT_FDCWD,
            descriptor_path.as_str(),
            AT_FDCWD,
            path,
            AtFlags::AT_SYMLINK_FOLLOW,
        )
        .map_err(|source| match source {
            Errno::EEXIST => Error::AlreadyExists {
                path: path.to_path_buf(),
            },
            _ => Error::Link {
                path: path.to_path_buf(),
                source,
            },
        })
    }
}

impl Write for UnnamedFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.file.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Fails with `Error::AlreadyExists` when something already exists at `to`.
pub fn rename_without_replace(from: &Path, to: &Path) -> Result<(), Error> {
    nix::fcntl::renameat2(AT_FDCWD, from, AT_FDCWD, to, RenameFlags::RENAME_NOREPLACE).map_err(
        |source| match source {
            Errno::EEXIST | Errno::ENOTEMPTY => Error::AlreadyExists {
                path: to.to_path_buf(),
            },
            _ => Error::Rename {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                source,
            },
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::{ErrorKind, Write};
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::{Error, UnnamedFile, rename_without_replace};

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    /// Maps each path under `directory`, relative to it, to the file's content (`None` for a directory).
    fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        let mut entries = BTreeMap::new();
        add_entries(directory, Path::new(""), &mut entries);
        entries
    }

    fn add_entries(root: &Path, relative: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                add_entries(root, &path, entries);
                entries.insert(path, None);
            } else {
                entries.insert(path, Some(fs::read(entry.path()).unwrap()));
            }
        }
    }

    fn read_if_exists(path: &Path) -> Option<Vec<u8>> {
        match fs::read(path) {
            Ok(content) => Some(content),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => panic!("failed to read {}: {error}", path.display()),
        }
    }

    fn unnamed_file_with_content(directory: &Path, content: &[u8]) -> UnnamedFile {
        let mut file = UnnamedFile::create_in(directory).unwrap();
        file.write_all(content).unwrap();
        file
    }

    fn create_directory_with_file(path: &Path) {
        fs::create_dir(path).unwrap();
        fs::write(path.join("eula.txt"), b"eula=true\n").unwrap();
    }

    /// d-fs-1
    #[test]
    fn written_unnamed_file_has_no_name_in_its_directory() {
        let directory = temporary_directory();
        let before = snapshot(directory.path());

        // The file stays open during the snapshot: had the process been killed at this point,
        // no name would have been left behind either.
        let _file = unnamed_file_with_content(directory.path(), b"written, never linked");

        assert_eq!(snapshot(directory.path()), before);
    }

    /// d-fs-2
    #[test]
    fn link_target_is_absent_while_writing_and_complete_after_linking() {
        let directory = temporary_directory();
        let path = directory.path().join("server.jar");
        let mut file = UnnamedFile::create_in(directory.path()).unwrap();
        let mut observations = Vec::new();

        for piece in [b"first ".as_slice(), b"second ", b"third"] {
            file.write_all(piece).unwrap();
            observations.push(read_if_exists(&path));
        }
        file.link_as(&path).unwrap();
        observations.push(read_if_exists(&path));

        assert_eq!(
            observations,
            [None, None, None, Some(b"first second third".to_vec())]
        );
    }

    /// Relied on by d-cache-3: linking onto an existing path fails with `AlreadyExists`.
    #[test]
    fn link_as_onto_an_existing_path_reports_already_exists() {
        let directory = temporary_directory();
        let path = directory.path().join("server.jar");
        fs::write(&path, b"existing").unwrap();
        let file = unnamed_file_with_content(directory.path(), b"new");

        let result = file.link_as(&path);

        assert!(matches!(result, Err(Error::AlreadyExists { path: reported }) if reported == path));
    }

    /// Relied on by d-cache-2: linking onto an existing path leaves the existing file unchanged.
    #[test]
    fn link_as_onto_an_existing_path_keeps_the_existing_file() {
        let directory = temporary_directory();
        let path = directory.path().join("server.jar");
        fs::write(&path, b"existing").unwrap();
        let file = unnamed_file_with_content(directory.path(), b"new");
        let before = snapshot(directory.path());

        file.link_as(&path).unwrap_err();

        assert_eq!(snapshot(directory.path()), before);
    }

    /// Relied on by d-new-1: the renamed directory keeps its contents.
    #[test]
    fn rename_without_replace_moves_a_directory_with_its_contents() {
        let parent = temporary_directory();
        let from = parent.path().join("from");
        create_directory_with_file(&from);

        rename_without_replace(&from, &parent.path().join("to")).unwrap();

        assert_eq!(
            snapshot(parent.path()),
            BTreeMap::from([
                (PathBuf::from("to"), None),
                (PathBuf::from("to/eula.txt"), Some(b"eula=true\n".to_vec())),
            ])
        );
    }

    /// Relied on by d-new-2: renaming onto an existing directory fails with `AlreadyExists`.
    #[test]
    fn rename_without_replace_onto_an_existing_directory_reports_already_exists() {
        let parent = temporary_directory();
        let from = parent.path().join("from");
        create_directory_with_file(&from);
        // A plain rename would replace an empty directory, so only RENAME_NOREPLACE makes this fail.
        let to = parent.path().join("to");
        fs::create_dir(&to).unwrap();

        let result = rename_without_replace(&from, &to);

        assert!(matches!(result, Err(Error::AlreadyExists { path }) if path == to));
    }

    /// Relied on by d-new-2: renaming onto an existing directory leaves both directories unchanged.
    #[test]
    fn rename_without_replace_onto_an_existing_directory_keeps_both_directories() {
        let parent = temporary_directory();
        let from = parent.path().join("from");
        create_directory_with_file(&from);
        let to = parent.path().join("to");
        fs::create_dir(&to).unwrap();
        let before = snapshot(parent.path());

        rename_without_replace(&from, &to).unwrap_err();

        assert_eq!(snapshot(parent.path()), before);
    }
}
