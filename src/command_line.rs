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
    Plugin(PluginArguments),
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
            RawSubcommand::Plugin(arguments) => Subcommand::Plugin(arguments),
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
    /// Manage the plugins of the server in the current directory
    Plugin(PluginArguments),
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

// The plugin subcommands take every value as a string, which their steps check, so that a value
// of the wrong form is an error with exit status 1 (err-9) rather than clap's 2.

#[derive(Debug, clap::Args)]
pub struct PluginArguments {
    #[command(subcommand)]
    pub subcommand: PluginSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum PluginSubcommand {
    /// Search Modrinth for plugins
    Search(SearchArguments),
    /// List the Modrinth versions of a plugin
    Versions(PluginVersionsArguments),
    /// Fetch a plugin's jar into the cache directory
    Fetch(FetchArguments),
    /// Show what a plugin's jar declares and how it fits the server
    Inspect(InspectArguments),
    /// Place a fetched plugin's jar in plugins/ of the server
    Place(PlaceArguments),
    /// Fetch, inspect, and place a plugin's jar
    Install(InstallArguments),
    /// List the plugins of the server, or with --global the fetched ones
    List(ListArguments),
    /// Remove a placed plugin from the server
    Remove(RemoveArguments),
}

#[derive(Debug, clap::Args)]
pub struct SearchArguments {
    /// Words to search Modrinth for
    pub term: String,
    /// How many plugins to show, from 10 to 100 (default: 10)
    #[arg(long, value_name = "COUNT")]
    pub limit: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct PluginVersionsArguments {
    /// The plugin on Modrinth
    #[arg(value_name = "modrinth:IDENTIFIER")]
    pub plugin: String,
}

#[derive(Debug, clap::Args)]
pub struct FetchArguments {
    /// modrinth:<identifier>[:<value>...], a jar URL, or the path of a local jar file
    pub source: String,
    /// Use the plugin name made from a URL without asking
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, clap::Args)]
pub struct InspectArguments {
    /// modrinth:<identifier>[:<value>...], a jar URL, or the path of a local jar file
    pub source: String,
    /// Use the plugin name made from a URL without asking
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, clap::Args)]
pub struct PlaceArguments {
    /// The source the jar was fetched with, or the path of a local jar file
    pub source: String,
}

#[derive(Debug, clap::Args)]
pub struct InstallArguments {
    /// modrinth:<identifier>[:<value>...], a jar URL, or the path of a local jar file
    pub source: String,
    /// For modrinth:<identifier> alone, take the newest version that matches the server
    #[arg(long)]
    pub lazy: bool,
    /// Place the jar without asking, and use the plugin name made from a URL
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, clap::Args)]
pub struct ListArguments {
    /// List the plugins in the cache directory instead
    #[arg(long)]
    pub global: bool,
}

#[derive(Debug, clap::Args)]
pub struct RemoveArguments {
    /// The plugin name in the record of mism.toml
    pub name: String,
}
