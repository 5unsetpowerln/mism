use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Software {
    Paper,
    Folia,
}

impl fmt::Display for Software {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            Software::Paper => "paper",
            Software::Folia => "folia",
        })
    }
}

impl FromStr for Software {
    type Err = ();

    fn from_str(text: &str) -> Result<Software, ()> {
        match text {
            "paper" => Ok(Software::Paper),
            "folia" => Ok(Software::Folia),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub struct CommandLine {
    pub subcommand: Subcommand,
}

#[derive(Debug)]
pub enum Subcommand {
    New(NewArguments),
    Versions(VersionsArguments),
}

#[derive(Debug)]
pub struct NewArguments {
    pub software: Software,
    pub path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug)]
pub struct VersionsArguments {
    pub software: Software,
}

impl CommandLine {
    /// Parses the process arguments, or prints the usage error or help and exits.
    pub fn parse_or_exit() -> CommandLine {
        let subcommand = match RawCommandLine::parse().subcommand {
            RawSubcommand::New(raw) => Subcommand::New(NewArguments {
                software: parse_software("new", &raw.software),
                path: raw.path,
                version: raw.version,
            }),
            RawSubcommand::Versions(raw) => Subcommand::Versions(VersionsArguments {
                software: parse_software("versions", &raw.software),
            }),
        };
        CommandLine { subcommand }
    }
}

// clap's own value validation omits the usage line from its error, so the software name is
// validated here and reported through the subcommand's clap error (cli-5).
fn parse_software(subcommand_name: &str, text: &str) -> Software {
    text.parse().unwrap_or_else(|()| {
        let mut command = RawCommandLine::command();
        // Building propagates the binary name, so the usage reads `mism <subcommand> ...`.
        command.build();
        command
            .find_subcommand_mut(subcommand_name)
            .expect("the subcommand name is one of the defined subcommands")
            .error(
                ErrorKind::InvalidValue,
                format!("invalid value '{text}' for '<SOFTWARE>': expected 'paper' or 'folia'"),
            )
            .exit()
    })
}

/// Minecraft server manager for Paper and Folia
#[derive(Debug, Parser)]
#[command(name = "mism", version)]
struct RawCommandLine {
    #[command(subcommand)]
    subcommand: RawSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum RawSubcommand {
    /// Create a new server directory
    New(RawNewArguments),
    /// List the Minecraft versions the server software is distributed for
    Versions(RawVersionsArguments),
}

#[derive(Debug, clap::Args)]
struct RawNewArguments {
    /// Server software to install: paper or folia
    software: String,
    /// Path of the server directory to create
    path: PathBuf,
    /// Minecraft version to install (default: the latest supported version)
    #[arg(long, value_name = "MINECRAFT_VERSION")]
    version: Option<String>,
}

#[derive(Debug, clap::Args)]
struct RawVersionsArguments {
    /// Server software to list versions for: paper or folia
    software: String,
}
