use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupportStatus {
    Supported,
    Deprecated,
    Unsupported,
}

impl fmt::Display for SupportStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            SupportStatus::Supported => "SUPPORTED",
            SupportStatus::Deprecated => "DEPRECATED",
            SupportStatus::Unsupported => "UNSUPPORTED",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    pub minecraft_version: String,
    pub support_status: SupportStatus,
    /// The PaperMC API calls the software versions "builds".
    pub software_versions: Vec<u64>,
    pub minimum_java_version: u32,
}

#[derive(Debug, Deserialize)]
pub struct VersionList {
    pub versions: Vec<VersionEntry>,
}

impl<'de> Deserialize<'de> for VersionEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Entry {
            version: Version,
            builds: Vec<u64>,
        }
        #[derive(Deserialize)]
        struct Version {
            id: String,
            support: Support,
            java: Java,
        }
        #[derive(Deserialize)]
        struct Support {
            status: SupportStatus,
        }
        #[derive(Deserialize)]
        struct Java {
            version: JavaVersion,
        }
        #[derive(Deserialize)]
        struct JavaVersion {
            minimum: u32,
        }

        let entry = Entry::deserialize(deserializer)?;
        Ok(VersionEntry {
            minecraft_version: entry.version.id,
            support_status: entry.version.support.status,
            software_versions: entry.builds,
            minimum_java_version: entry.version.java.version.minimum,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub name: String,
    pub sha256: String,
    pub url: String,
}

impl<'de> Deserialize<'de> for Download {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Response {
            downloads: HashMap<String, Artifact>,
        }
        #[derive(Deserialize)]
        struct Artifact {
            name: String,
            checksums: Checksums,
            url: String,
        }
        #[derive(Deserialize)]
        struct Checksums {
            sha256: String,
        }

        let mut response = Response::deserialize(deserializer)?;
        let artifact = response
            .downloads
            .remove("server:default")
            .ok_or_else(|| serde::de::Error::missing_field("server:default"))?;
        Ok(Download {
            name: artifact.name,
            sha256: artifact.checksums.sha256,
            url: artifact.url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER_VERSIONS: &str = include_str!("../../tests/fixtures/paper-versions.json");
    const FOLIA_VERSIONS: &str = include_str!("../../tests/fixtures/folia-versions.json");
    const PAPER_DOWNLOAD: &str = include_str!("../../tests/fixtures/paper-26.3-206.json");
    const FOLIA_DOWNLOAD: &str = include_str!("../../tests/fixtures/folia-26.3-3.json");

    fn entry_26_3(json: &str) -> VersionEntry {
        serde_json::from_str::<VersionList>(json)
            .unwrap()
            .versions
            .into_iter()
            .find(|entry| entry.minecraft_version == "26.3")
            .unwrap()
    }

    /// relates to sel-1: the support status is read from `version.support.status`.
    #[test]
    fn paper_version_list_reports_26_3_as_supported() {
        assert_eq!(
            entry_26_3(PAPER_VERSIONS).support_status,
            SupportStatus::Supported
        );
    }

    /// relates to sel-3: the software versions are read from `builds`.
    #[test]
    fn paper_version_list_contains_software_version_206_for_26_3() {
        assert!(entry_26_3(PAPER_VERSIONS).software_versions.contains(&206));
    }

    #[test]
    fn paper_version_list_reports_java_25_for_26_3() {
        assert_eq!(entry_26_3(PAPER_VERSIONS).minimum_java_version, 25);
    }

    #[test]
    fn folia_version_list_deserializes() {
        assert!(serde_json::from_str::<VersionList>(FOLIA_VERSIONS).is_ok());
    }

    #[test]
    fn folia_version_list_contains_software_version_3_for_26_3() {
        assert!(entry_26_3(FOLIA_VERSIONS).software_versions.contains(&3));
    }

    #[test]
    fn paper_download_name_is_the_jar_name() {
        let download: Download = serde_json::from_str(PAPER_DOWNLOAD).unwrap();
        assert_eq!(download.name, "paper-26.3-206.jar");
    }

    #[test]
    fn paper_download_sha256_is_read_from_checksums() {
        let download: Download = serde_json::from_str(PAPER_DOWNLOAD).unwrap();
        assert_eq!(
            download.sha256,
            "4208a692538104ec88699c0e078b2b89842920a10472ebb37e72cdda27c0d80e"
        );
    }

    #[test]
    fn paper_download_url_points_to_the_object_store() {
        let download: Download = serde_json::from_str(PAPER_DOWNLOAD).unwrap();
        assert!(download.url.starts_with("https://fill-data.papermc.io/"));
    }

    #[test]
    fn folia_download_name_is_the_jar_name() {
        let download: Download = serde_json::from_str(FOLIA_DOWNLOAD).unwrap();
        assert_eq!(download.name, "folia-26.3-3.jar");
    }

    #[test]
    fn download_without_server_default_is_rejected() {
        assert!(serde_json::from_str::<Download>(r#"{"downloads":{}}"#).is_err());
    }

    #[test]
    fn unknown_support_status_is_rejected() {
        assert!(serde_json::from_str::<SupportStatus>(r#""EXPERIMENTAL""#).is_err());
    }

    #[test]
    fn support_status_displays_its_api_spelling() {
        assert_eq!(SupportStatus::Deprecated.to_string(), "DEPRECATED");
    }
}
