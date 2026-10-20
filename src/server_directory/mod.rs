pub mod creation;
mod initial_launch;
pub mod mism_toml;

use std::path::PathBuf;

use mism_toml::MismToml;

/// A server directory that `mism new` created: where it is, and what its `mism.toml` records.
pub struct ServerDirectory {
    pub path: PathBuf,
    pub mism_toml: MismToml,
}
