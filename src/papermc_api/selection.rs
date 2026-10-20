use super::response::{Download, SupportStatus, VersionEntry};
use super::{Error, PaperMcApi};
use crate::minecraft_version::MinecraftVersion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub minecraft_version: String,
    pub software_version: u64,
    pub minimum_java_version: u32,
    pub download: Download,
}

/// Fetches the version list once, picks a version from it, and fetches its download once.
pub fn select(
    api: &dyn PaperMcApi,
    project: &str,
    requested: Option<&str>,
) -> Result<Selected, Error> {
    let entries = api.minecraft_versions(project)?;
    let entry = match requested {
        Some(requested) => find_requested(&entries, project, requested)?,
        None => find_latest_supported(&entries, project)?,
    };
    let software_version = newest_software_version(entry, project)?;
    // The URL gets the id of the matched entry, never the user's text.
    let download = api.download(project, &entry.minecraft_version, software_version)?;
    Ok(Selected {
        minecraft_version: entry.minecraft_version.clone(),
        software_version,
        minimum_java_version: entry.minimum_java_version,
        download,
    })
}

fn find_requested<'a>(
    entries: &'a [VersionEntry],
    project: &str,
    requested: &str,
) -> Result<&'a VersionEntry, Error> {
    entries
        .iter()
        .find(|entry| entry.minecraft_version == requested)
        .ok_or_else(|| Error::MinecraftVersionNotDistributed {
            requested: requested.to_owned(),
            project: project.to_owned(),
        })
}

fn find_latest_supported<'a>(
    entries: &'a [VersionEntry],
    project: &str,
) -> Result<&'a VersionEntry, Error> {
    let mut supported = Vec::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.support_status == SupportStatus::Supported)
    {
        supported.push((MinecraftVersion::parse(&entry.minecraft_version)?, entry));
    }
    supported
        .into_iter()
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, entry)| entry)
        .ok_or_else(|| Error::NoSupportedMinecraftVersion {
            project: project.to_owned(),
        })
}

fn newest_software_version(entry: &VersionEntry, project: &str) -> Result<u64, Error> {
    entry
        .software_versions
        .iter()
        .max()
        .copied()
        .ok_or_else(|| Error::NoSoftwareVersion {
            project: project.to_owned(),
            minecraft_version: entry.minecraft_version.clone(),
        })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct FakeApi {
        entries: Vec<VersionEntry>,
        calls: RefCell<Vec<String>>,
    }

    impl FakeApi {
        fn new(entries: Vec<VersionEntry>) -> FakeApi {
            FakeApi {
                entries,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl PaperMcApi for FakeApi {
        fn minecraft_versions(&self, project: &str) -> Result<Vec<VersionEntry>, Error> {
            self.calls.borrow_mut().push(format!("list {project}"));
            Ok(self.entries.clone())
        }

        fn download(
            &self,
            project: &str,
            minecraft_version: &str,
            software_version: u64,
        ) -> Result<Download, Error> {
            self.calls.borrow_mut().push(format!(
                "download {project} {minecraft_version} {software_version}"
            ));
            Ok(Download {
                name: "server.jar".to_owned(),
                sha256: "00".to_owned(),
                url: "http://example.invalid/server.jar".to_owned(),
            })
        }
    }

    fn entry(id: &str, status: SupportStatus, builds: &[u64]) -> VersionEntry {
        VersionEntry {
            minecraft_version: id.to_owned(),
            support_status: status,
            software_versions: builds.to_vec(),
            minimum_java_version: 21,
        }
    }

    fn supported(id: &str, builds: &[u64]) -> VersionEntry {
        entry(id, SupportStatus::Supported, builds)
    }

    /// relates to sel-1, d-api-3: release order wins over list order.
    #[test]
    fn latest_supported_follows_release_order_not_list_order() {
        let api = FakeApi::new(vec![
            supported("1.21.4", &[1]),
            supported("26.3", &[2]),
            supported("1.21.10", &[3]),
        ]);
        assert_eq!(
            select(&api, "paper", None).unwrap().minecraft_version,
            "26.3"
        );
    }

    /// relates to sel-1: only SUPPORTED entries are candidates.
    #[test]
    fn latest_supported_skips_deprecated_newer_versions() {
        let api = FakeApi::new(vec![
            entry("26.4", SupportStatus::Deprecated, &[1]),
            supported("26.3", &[1]),
        ]);
        assert_eq!(
            select(&api, "paper", None).unwrap().minecraft_version,
            "26.3"
        );
    }

    /// relates to err-4
    #[test]
    fn omitted_version_without_supported_entry_is_an_error() {
        let api = FakeApi::new(vec![entry("26.3", SupportStatus::Deprecated, &[1])]);
        assert!(matches!(
            select(&api, "paper", None),
            Err(Error::NoSupportedMinecraftVersion { .. })
        ));
    }

    #[test]
    fn omitted_version_with_unparsable_supported_id_is_an_error() {
        let api = FakeApi::new(vec![supported("latest", &[1])]);
        assert!(matches!(
            select(&api, "paper", None),
            Err(Error::MinecraftVersion(_))
        ));
    }

    /// relates to sel-2
    #[test]
    fn requested_version_matches_exactly() {
        let api = FakeApi::new(vec![entry("1.21.4", SupportStatus::Unsupported, &[5])]);
        assert_eq!(
            select(&api, "paper", Some("1.21.4"))
                .unwrap()
                .minecraft_version,
            "1.21.4"
        );
    }

    /// relates to sel-2, err-3: `26.03` does not match `26.3`.
    #[test]
    fn requested_version_is_not_normalized() {
        let api = FakeApi::new(vec![supported("26.3", &[1])]);
        assert!(matches!(
            select(&api, "paper", Some("26.03")),
            Err(Error::MinecraftVersionNotDistributed { .. })
        ));
    }

    /// relates to err-3
    #[test]
    fn not_distributed_error_hints_at_the_versions_subcommand() {
        let api = FakeApi::new(vec![]);
        let error = select(&api, "folia", Some("1.0")).unwrap_err();
        assert_eq!(
            error.hint().unwrap(),
            "run `mism versions folia` to see the distributed Minecraft versions"
        );
    }

    /// relates to sel-3
    #[test]
    fn software_version_is_the_maximum_of_builds() {
        let api = FakeApi::new(vec![supported("26.3", &[4, 206, 7])]);
        assert_eq!(select(&api, "paper", None).unwrap().software_version, 206);
    }

    #[test]
    fn empty_builds_is_an_error() {
        let api = FakeApi::new(vec![supported("26.3", &[])]);
        assert!(matches!(
            select(&api, "paper", None),
            Err(Error::NoSoftwareVersion { .. })
        ));
    }

    #[test]
    fn minimum_java_version_comes_from_the_matched_entry() {
        let mut matched = supported("26.3", &[1]);
        matched.minimum_java_version = 25;
        let api = FakeApi::new(vec![matched]);
        assert_eq!(
            select(&api, "paper", None).unwrap().minimum_java_version,
            25
        );
    }

    /// d-api-1
    #[test]
    fn selection_calls_the_api_exactly_twice() {
        let api = FakeApi::new(vec![supported("26.3", &[1])]);
        select(&api, "paper", None).unwrap();
        assert_eq!(api.calls.borrow().len(), 2);
    }

    /// d-api-2: the download call receives the id of the matched entry and the maximum of its builds.
    #[test]
    fn download_is_requested_with_the_matched_id() {
        let api = FakeApi::new(vec![supported("1.21.4", &[1]), supported("26.3", &[3, 9])]);
        select(&api, "paper", Some("26.3")).unwrap();
        assert_eq!(api.calls.borrow()[1], "download paper 26.3 9");
    }
}
