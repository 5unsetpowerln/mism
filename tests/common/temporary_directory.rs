use std::fs;
use std::path::Path;

use tempfile::TempDir;

/// Creates a fresh temporary directory under `<CARGO_MANIFEST_DIR>/target/test-tmp`.
pub fn create() -> TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
    fs::create_dir_all(&parent).unwrap();
    TempDir::new_in(parent).unwrap()
}
