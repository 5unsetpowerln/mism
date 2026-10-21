pub mod response;
pub mod selection;

use std::env;

use crate::http_client::{self, HttpClient};
use crate::interruption::Signal;
use crate::minecraft_version;

const DEFAULT_BASE_URL: &str = "https://fill.papermc.io/v3";
const BASE_URL_VARIABLE: &str = "MISM_PAPERMC_URL";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] http_client::Error),
    #[error(transparent)]
    MinecraftVersion(#[from] minecraft_version::Error),
    #[error("PaperMC does not distribute {project} for Minecraft {requested}")]
    MinecraftVersionNotDistributed { requested: String, project: String },
    #[error("PaperMC lists no supported Minecraft version for {project}")]
    NoSupportedMinecraftVersion { project: String },
    #[error("PaperMC distributes no {project} version for Minecraft {minecraft_version}")]
    NoSoftwareVersion {
        project: String,
        minecraft_version: String,
    },
}

impl Error {
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Http(error) => error.hint(),
            Error::MinecraftVersion(error) => error.hint(),
            Error::MinecraftVersionNotDistributed { project, .. } => Some(format!(
                "run `mism versions {project}` to see the distributed Minecraft versions"
            )),
            Error::NoSupportedMinecraftVersion { .. } | Error::NoSoftwareVersion { .. } => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Http(error) => error.interrupted_by(),
            Error::MinecraftVersion(_)
            | Error::MinecraftVersionNotDistributed { .. }
            | Error::NoSupportedMinecraftVersion { .. }
            | Error::NoSoftwareVersion { .. } => None,
        }
    }
}

pub trait PaperMcApi {
    fn minecraft_versions(&self, project: &str) -> Result<Vec<response::VersionEntry>, Error>;

    fn download(
        &self,
        project: &str,
        minecraft_version: &str,
        software_version: u64,
    ) -> Result<response::Download, Error>;
}

pub struct HttpPaperMcApi {
    client: HttpClient,
    base_url: String,
}

impl HttpPaperMcApi {
    pub fn new(client: HttpClient) -> HttpPaperMcApi {
        let base_url = env::var(BASE_URL_VARIABLE).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        HttpPaperMcApi::with_base_url(client, &base_url)
    }

    fn with_base_url(client: HttpClient, base_url: &str) -> HttpPaperMcApi {
        HttpPaperMcApi {
            client,
            base_url: base_url.strip_suffix('/').unwrap_or(base_url).to_owned(),
        }
    }
}

impl PaperMcApi for HttpPaperMcApi {
    fn minecraft_versions(&self, project: &str) -> Result<Vec<response::VersionEntry>, Error> {
        let url = format!("{}/projects/{project}/versions", self.base_url);
        let list: response::VersionList = self.client.get_json(&url, &[])?;
        Ok(list.versions)
    }

    fn download(
        &self,
        project: &str,
        minecraft_version: &str,
        software_version: u64,
    ) -> Result<response::Download, Error> {
        let url = format!(
            "{}/projects/{project}/versions/{minecraft_version}/builds/{software_version}",
            self.base_url
        );
        Ok(self.client.get_json(&url, &[])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(base_url: &str) -> HttpPaperMcApi {
        HttpPaperMcApi::with_base_url(HttpClient::new().unwrap(), base_url)
    }

    #[test]
    fn one_trailing_slash_is_stripped_from_the_base_url() {
        assert_eq!(
            api("http://127.0.0.1:1/v3/").base_url,
            "http://127.0.0.1:1/v3"
        );
    }

    #[test]
    fn base_url_without_trailing_slash_is_kept() {
        assert_eq!(
            api("http://127.0.0.1:1/v3").base_url,
            "http://127.0.0.1:1/v3"
        );
    }
}
