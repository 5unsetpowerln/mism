pub mod creation;
mod initial_launch;
pub mod mmmm_toml;

use std::path::PathBuf;

use mmmm_toml::MmmmToml;

/// A server directory that `mmmm new` created: where it is, and what its `mmmm.toml` records.
pub struct ServerDirectory {
    pub path: PathBuf,
    pub mmmm_toml: MmmmToml,
}
