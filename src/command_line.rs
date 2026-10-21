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
    Datapack(DatapackArguments),
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
            RawSubcommand::Datapack(arguments) => Subcommand::Datapack(arguments),
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
    /// Manage the datapacks of the world of the server in the current directory
    Datapack(DatapackArguments),
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

// The subcommands of `plugin` and `datapack` take every value as a string, which their steps
// check, so that a value of the wrong form is an error with exit status 1 (err-9) rather than
// clap's 2.

#[derive(Debug, clap::Args)]
pub struct PluginArguments {
    #[command(subcommand)]
    pub subcommand: PluginSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum PluginSubcommand {
    /// Search Modrinth for plugins
    Search(PluginSearchArguments),
    /// List the Modrinth versions of a plugin
    Versions(PluginVersionsArguments),
    /// Fetch a plugin's jar into the cache directory
    Fetch(PluginFetchArguments),
    /// Show what a plugin's jar declares and how it fits the server
    Inspect(PluginInspectArguments),
    /// Place a fetched plugin's jar in plugins/ of the server
    Place(PluginPlaceArguments),
    /// Fetch, inspect, and place a plugin's jar
    Install(PluginInstallArguments),
    /// List the plugins of the server, or with --global the fetched ones
    List(PluginListArguments),
    /// Remove a placed plugin from the server
    Remove(PluginRemoveArguments),
}

#[derive(Debug, clap::Args)]
pub struct PluginSearchArguments {
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
pub struct PluginFetchArguments {
    /// modrinth:<identifier>[:<value>...], a jar URL, or the path of a local jar file
    pub source: String,
    /// Use the plugin name made from a URL without asking
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, clap::Args)]
pub struct PluginInspectArguments {
    /// modrinth:<identifier>[:<value>...], a jar URL, or the path of a local jar file
    pub source: String,
    /// Use the plugin name made from a URL without asking
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, clap::Args)]
pub struct PluginPlaceArguments {
    /// The source the jar was fetched with, or the path of a local jar file
    pub source: String,
}

#[derive(Debug, clap::Args)]
pub struct PluginInstallArguments {
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
pub struct PluginListArguments {
    /// List the plugins in the cache directory instead
    #[arg(long)]
    pub global: bool,
}

#[derive(Debug, clap::Args)]
pub struct PluginRemoveArguments {
    /// The plugin name in the record of mism.toml
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct DatapackArguments {
    #[command(subcommand)]
    pub subcommand: DatapackSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum DatapackSubcommand {
    /// Search Modrinth for datapacks
    Search(DatapackSearchArguments),
    /// List the Modrinth versions of a datapack
    Versions(DatapackVersionsArguments),
    /// Fetch a datapack file into the cache directory
    Fetch(DatapackFetchArguments),
    /// Show what a datapack file declares and how it fits the server
    Inspect(DatapackInspectArguments),
    /// Place a fetched datapack file in datapacks/ of the server's world
    Place(DatapackPlaceArguments),
    /// Fetch, inspect, and place a datapack file
    Install(DatapackInstallArguments),
    /// List the datapacks of the server's world, or with --global the fetched ones
    List(DatapackListArguments),
    /// Remove a placed datapack from the server's world
    Remove(DatapackRemoveArguments),
}

#[derive(Debug, clap::Args)]
pub struct DatapackSearchArguments {
    /// Words to search Modrinth for
    pub term: String,
    /// How many datapacks to show, from 10 to 100 (default: 10)
    #[arg(long, value_name = "COUNT")]
    pub limit: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct DatapackVersionsArguments {
    /// The datapack on Modrinth
    #[arg(value_name = "modrinth:IDENTIFIER")]
    pub datapack: String,
}

#[derive(Debug, clap::Args)]
pub struct DatapackFetchArguments {
    /// modrinth:<identifier>[:<value>...], a datapack file URL, or the path of a local datapack
    /// file
    pub source: String,
    /// Ask nothing: use the datapack name made from a URL, and fail where a Modrinth value is
    /// left to choose
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, clap::Args)]
pub struct DatapackInspectArguments {
    /// modrinth:<identifier>[:<value>...], a datapack file URL, or the path of a local datapack
    /// file
    pub source: String,
    /// Ask nothing: use the datapack name made from a URL, and fail where a Modrinth value is
    /// left to choose
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, clap::Args)]
pub struct DatapackPlaceArguments {
    /// The source the datapack file was fetched with, or the path of a local datapack file
    pub source: String,
}

#[derive(Debug, clap::Args)]
pub struct DatapackInstallArguments {
    /// modrinth:<identifier>[:<value>...], a datapack file URL, or the path of a local datapack
    /// file
    pub source: String,
    /// For modrinth:<identifier> alone, take the newest version that matches the server's
    /// Minecraft version
    #[arg(long)]
    pub lazy: bool,
    /// Ask nothing: place the datapack file, use the datapack name made from a URL, and fail
    /// where a Modrinth value is left to choose
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, clap::Args)]
pub struct DatapackListArguments {
    /// List the datapacks in the cache directory instead
    #[arg(long)]
    pub global: bool,
}

#[derive(Debug, clap::Args)]
pub struct DatapackRemoveArguments {
    /// The datapack name in the record of mism.toml
    pub name: String,
}
