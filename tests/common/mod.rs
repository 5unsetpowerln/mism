// Each integration-test crate uses a different subset of this shared support.
#![allow(dead_code)]

pub mod fake_java;
pub mod fake_papermc;
pub mod mism_process;
pub mod new_server_environment;
pub mod slow_jar_server;
pub mod temporary_directory;
