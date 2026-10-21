use jiff::Timestamp;
use serde::Deserialize;

/// The response of the search endpoint.
#[derive(Debug, Deserialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
}

/// A project found by the search endpoint, in the order Modrinth returned.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SearchHit {
    pub project_id: String,
    /// The identifier (plg-3).
    pub slug: String,
    /// The plugin name (plg-5).
    pub title: String,
}

/// The response of the project endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
}

/// A Modrinth version, an element of the version list endpoint's response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Version {
    pub id: String,
    pub version_number: String,
    /// `release`, `beta`, or `alpha`.
    pub version_type: String,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    /// Read as an RFC 3339 time; any other text makes the response invalid JSON for mism.
    pub date_published: Timestamp,
    pub dependencies: Vec<Dependency>,
    pub files: Vec<VersionFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    pub dependency_type: String,
}

impl Dependency {
    pub fn is_required(&self) -> bool {
        self.dependency_type == "required"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VersionFile {
    pub filename: String,
    pub url: String,
    pub hashes: FileHashes,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileHashes {
    pub sha512: String,
}

/// An element of the response of the endpoint for several projects.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProjectSlug {
    pub id: String,
    pub slug: String,
}

/// An element of the response of the endpoint for several versions.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VersionProject {
    pub id: String,
    pub project_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAVED_VERSION_LISTS: [&str; 7] = [
        include_str!("../../tests/fixtures/modrinth-versions-luckperms.json"),
        include_str!("../../tests/fixtures/modrinth-versions-fastasyncworldedit.json"),
        include_str!("../../tests/fixtures/modrinth-versions-multiverse-portals.json"),
        include_str!("../../tests/fixtures/modrinth-versions-multiverse-netherportals.json"),
        include_str!("../../tests/fixtures/modrinth-versions-multiverse-signportals.json"),
        include_str!("../../tests/fixtures/modrinth-versions-locationlast.json"),
        include_str!("../../tests/fixtures/modrinth-versions-playerportals.json"),
    ];
    const SEARCH: &str =
        include_str!("../../tests/fixtures/modrinth-search-multiverse-portals.json");
    const PROJECT: &str = include_str!("../../tests/fixtures/modrinth-project-luckperms.json");
    const PROJECTS: &str =
        include_str!("../../tests/fixtures/modrinth-projects-multiverse-core.json");

    fn version_json(date_published: &str) -> String {
        format!(
            r#"{{"id":"a","version_number":"1.0","version_type":"release","loaders":["paper"],
                "game_versions":["26.3"],"date_published":"{date_published}",
                "dependencies":[],"files":[]}}"#
        )
    }

    /// 「Modrinth の API」 方法: every saved version list is read.
    #[test]
    fn saved_version_lists_are_read() {
        let failures: Vec<_> = SAVED_VERSION_LISTS
            .iter()
            .filter_map(|json| serde_json::from_str::<Vec<Version>>(json).err())
            .collect();
        assert!(failures.is_empty(), "{failures:?}");
    }

    /// 「Modrinth の API」 方法: the search hits are read in Modrinth's order.
    #[test]
    fn search_hits_are_read_in_order() {
        let results: SearchResults = serde_json::from_str(SEARCH).unwrap();
        let slugs: Vec<_> = results.hits.iter().map(|hit| hit.slug.as_str()).collect();
        assert_eq!(
            slugs,
            [
                "multiverse-portals",
                "multiverse-netherportals",
                "multiverse-signportals",
                "locationlast",
                "playerportals"
            ]
        );
    }

    /// 「Modrinth の API」 方法: the project endpoint gives the id, the slug, and the title.
    #[test]
    fn project_is_read() {
        let project: Project = serde_json::from_str(PROJECT).unwrap();
        assert_eq!(
            project,
            Project {
                id: "Vebnzrzj".to_owned(),
                slug: "luckperms".to_owned(),
                title: "LuckPerms".to_owned(),
            }
        );
    }

    /// 「Modrinth の API」 方法: the endpoint for several projects gives each id and slug.
    #[test]
    fn several_projects_are_read() {
        let projects: Vec<ProjectSlug> = serde_json::from_str(PROJECTS).unwrap();
        assert_eq!(
            projects,
            [ProjectSlug {
                id: "3wmN97b8".to_owned(),
                slug: "multiverse-core".to_owned(),
            }]
        );
    }

    /// 「Modrinth の API」 方法: publication times with and without fractions and offsets are
    /// compared as times.
    #[test]
    fn publication_times_compare_as_times() {
        let shorter: Version =
            serde_json::from_str(&version_json("2026-10-06T07:14:55.653Z")).unwrap();
        let longer: Version =
            serde_json::from_str(&version_json("2026-10-06T07:14:55.653185+00:00")).unwrap();
        assert!(shorter.date_published < longer.date_published);
    }

    /// 「Modrinth の API」 方法: a publication time that is not RFC 3339 makes the JSON invalid.
    #[test]
    fn publication_time_that_is_not_rfc_3339_is_rejected() {
        assert!(serde_json::from_str::<Version>(&version_json("yesterday")).is_err());
    }
}
