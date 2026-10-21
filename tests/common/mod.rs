// Each integration-test crate uses a different subset of this shared support.
#![allow(dead_code)]

pub mod datapack_server_directory;
pub mod fake_java;
pub mod fake_modrinth;
pub mod fake_papermc;
pub mod management_environment;
pub mod mism_process;
pub mod new_server_environment;
pub mod plugin_output;
pub mod plugin_server_directory;
pub mod pseudo_terminal;
pub mod server_output;
pub mod slow_jar_server;
pub mod stdin_connection;
pub mod temporary_directory;
pub mod test_datapack_zip;
pub mod test_plugin_jar;
