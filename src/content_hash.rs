use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256, Sha512};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Read { .. } => None,
        }
    }

    /// Whether the file to hash does not exist.
    pub fn is_not_found(&self) -> bool {
        match self {
            Error::Read { source, .. } => source.kind() == io::ErrorKind::NotFound,
        }
    }
}

/// Writes to `inner` and hashes the bytes it wrote.
pub struct HashingWriter<W> {
    inner: W,
    sha256: Sha256,
    sha512: Option<Sha512>,
}

/// What a `HashingWriter` wrote to, and the lowercase hexadecimal digests of what it wrote.
pub struct Hashed<W> {
    pub inner: W,
    pub sha256: String,
    /// `None` unless the writer was made with `with_sha512`.
    pub sha512: Option<String>,
}

impl<W: Write> HashingWriter<W> {
    pub fn new(inner: W) -> HashingWriter<W> {
        HashingWriter {
            inner,
            sha256: Sha256::new(),
            sha512: None,
        }
    }

    pub fn with_sha512(inner: W) -> HashingWriter<W> {
        HashingWriter {
            sha512: Some(Sha512::new()),
            ..HashingWriter::new(inner)
        }
    }

    pub fn finish(self) -> Hashed<W> {
        Hashed {
            inner: self.inner,
            sha256: to_hex(&self.sha256.finalize()),
            sha512: self.sha512.map(|sha512| to_hex(&sha512.finalize())),
        }
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.sha256.update(&buffer[..written]);
        if let Some(sha512) = &mut self.sha512 {
            sha512.update(&buffer[..written]);
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The lowercase hexadecimal SHA-256 of the file at `path`.
pub fn sha256_of_file(path: &Path) -> Result<String, Error> {
    let read_failed = |source| Error::Read {
        path: path.to_path_buf(),
        source,
    };
    let mut file = File::open(path).map_err(read_failed)?;
    let mut hashing = HashingWriter::new(io::sink());
    io::copy(&mut file, &mut hashing).map_err(read_failed)?;
    Ok(hashing.finish().sha256)
}

fn to_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    use tempfile::TempDir;

    use super::{HashingWriter, sha256_of_file};

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const ABC_SHA512: &str = "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f";

    fn temporary_directory() -> TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }

    fn written_in_pieces(mut writer: HashingWriter<Vec<u8>>) -> super::Hashed<Vec<u8>> {
        writer.write_all(b"a").unwrap();
        writer.write_all(b"bc").unwrap();
        writer.finish()
    }

    /// 「キャッシュのディレクトリとハッシュの計算」 方法: the writer passes the content on.
    #[test]
    fn hashing_writer_writes_the_content_to_its_inner_writer() {
        assert_eq!(
            written_in_pieces(HashingWriter::new(Vec::new())).inner,
            b"abc"
        );
    }

    /// 「キャッシュのディレクトリとハッシュの計算」 方法: SHA-256 in lowercase hexadecimal.
    #[test]
    fn hashing_writer_gives_the_sha256_in_lowercase_hexadecimal() {
        assert_eq!(
            written_in_pieces(HashingWriter::new(Vec::new())).sha256,
            ABC_SHA256
        );
    }

    /// 「キャッシュのディレクトリとハッシュの計算」 方法: SHA-512 when the caller asks for it.
    #[test]
    fn hashing_writer_with_sha512_gives_the_sha512() {
        assert_eq!(
            written_in_pieces(HashingWriter::with_sha512(Vec::new())).sha512,
            Some(ABC_SHA512.to_owned())
        );
    }

    /// 「キャッシュのディレクトリとハッシュの計算」 方法: no SHA-512 unless the caller asks.
    #[test]
    fn hashing_writer_without_sha512_gives_none() {
        assert_eq!(
            written_in_pieces(HashingWriter::new(Vec::new())).sha512,
            None
        );
    }

    /// 「キャッシュのディレクトリとハッシュの計算」 方法: the SHA-256 of an existing file.
    #[test]
    fn sha256_of_file_hashes_the_file_content() {
        let directory = temporary_directory();
        let path = directory.path().join("plugin.jar");
        fs::write(&path, b"abc").unwrap();

        assert_eq!(sha256_of_file(&path).unwrap(), ABC_SHA256);
    }

    /// 「キャッシュのディレクトリとハッシュの計算」 方法: a missing file is told apart.
    #[test]
    fn sha256_of_a_missing_file_is_a_not_found_error() {
        let directory = temporary_directory();

        let error = sha256_of_file(&directory.path().join("missing.jar")).unwrap_err();

        assert!(error.is_not_found());
    }
}
