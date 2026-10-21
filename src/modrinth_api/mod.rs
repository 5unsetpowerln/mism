pub mod response;
pub mod selection;

use std::collections::{BTreeMap, BTreeSet};
use std::env;

use serde::de::DeserializeOwned;

use response::{
    Dependency, Project, ProjectSlug, SearchHit, SearchResults, Version, VersionProject,
};

use crate::http_client::{self, HttpClient};
use crate::interruption::{self, Signal};

const DEFAULT_BASE_URL: &str = "https://api.modrinth.com/v2";
const BASE_URL_VARIABLE: &str = "MISM_MODRINTH_URL";
const NOT_FOUND: u16 = 404;
const TOO_MANY_REQUESTS: u16 = 429;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{BASE_URL_VARIABLE} is not valid UTF-8")]
    BaseUrlNotUnicode,
    #[error(transparent)]
    Http(http_client::Error),
    #[error("Modrinth has no project with the identifier {identifier:?}")]
    ProjectNotFound {
        identifier: String,
        #[source]
        source: http_client::Error,
    },
    #[error(
        "{given:?} is not the identifier of the Modrinth project it names, whose identifier is {slug:?}"
    )]
    NotTheIdentifier { given: String, slug: String },
    #[error("Modrinth refused the request because too many requests were sent to it")]
    RateLimited(#[source] http_client::Error),
    #[error("Modrinth returned no version with the id {id}, which a required dependency names")]
    DependencyVersionNotReturned { id: String },
    #[error("Modrinth returned no project with the id {id}, which a required dependency names")]
    DependencyProjectNotReturned { id: String },
    #[error("interrupted by {0}")]
    Interrupted(Signal),
}

impl Error {
    /// None for a project that does not exist: its remedy names the search of the caller's
    /// subcommand.
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Http(error) => error.hint(),
            Error::NotTheIdentifier { slug, .. } => Some(format!(
                "use the identifier in the source: `modrinth:{slug}`"
            )),
            Error::RateLimited(_) => Some("wait a minute and run the command again".to_owned()),
            Error::BaseUrlNotUnicode
            | Error::ProjectNotFound { .. }
            | Error::DependencyVersionNotReturned { .. }
            | Error::DependencyProjectNotReturned { .. }
            | Error::Interrupted(_) => None,
        }
    }

    pub fn interrupted_by(&self) -> Option<Signal> {
        match self {
            Error::Interrupted(signal) => Some(*signal),
            Error::Http(error) => error.interrupted_by(),
            Error::BaseUrlNotUnicode
            | Error::ProjectNotFound { .. }
            | Error::NotTheIdentifier { .. }
            | Error::RateLimited(_)
            | Error::DependencyVersionNotReturned { .. }
            | Error::DependencyProjectNotReturned { .. } => None,
        }
    }

    fn from_http(error: http_client::Error) -> Error {
        match error {
            http_client::Error::Status {
                status: TOO_MANY_REQUESTS,
                ..
            } => Error::RateLimited(error),
            error => Error::Http(error),
        }
    }
}

/// The five Modrinth endpoints mism uses.
pub trait ModrinthApi {
    /// The projects found for `query` with the search filter `facets`, at most `limit`, in
    /// Modrinth's order (srch-1, dsrch-1).
    fn search(&self, query: &str, facets: &str, limit: u32) -> Result<Vec<SearchHit>, Error>;

    /// The project whose slug or id is `identifier`, compared without case by Modrinth.
    fn project(&self, identifier: &str) -> Result<Project, Error>;

    /// Every Modrinth version of the project with the id `project_id`.
    fn project_versions(&self, project_id: &str) -> Result<Vec<Version>, Error>;

    /// The projects among `ids` that exist.
    fn projects(&self, ids: &BTreeSet<String>) -> Result<Vec<ProjectSlug>, Error>;

    /// The Modrinth versions among `ids` that exist.
    fn versions(&self, ids: &BTreeSet<String>) -> Result<Vec<VersionProject>, Error>;
}

/// The Modrinth API over HTTP, at `MISM_MODRINTH_URL` or `https://api.modrinth.com/v2`.
pub struct HttpModrinthApi {
    client: HttpClient,
    base_url: String,
    /// `interruption::received`, except in unit tests, which must not touch the process-wide flag.
    received_signal: fn() -> Option<Signal>,
}

impl HttpModrinthApi {
    pub fn new(client: HttpClient) -> Result<HttpModrinthApi, Error> {
        let base_url = match env::var(BASE_URL_VARIABLE) {
            Ok(base_url) => base_url,
            Err(env::VarError::NotPresent) => DEFAULT_BASE_URL.to_owned(),
            Err(env::VarError::NotUnicode(_)) => return Err(Error::BaseUrlNotUnicode),
        };
        Ok(HttpModrinthApi::with_base_url(client, &base_url))
    }

    fn with_base_url(client: HttpClient, base_url: &str) -> HttpModrinthApi {
        HttpModrinthApi {
            client,
            base_url: base_url.strip_suffix('/').unwrap_or(base_url).to_owned(),
            received_signal: interruption::received,
        }
    }

    /// Sends no request once a signal has arrived (d-modrinth-1).
    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T, Error> {
        if let Some(signal) = (self.received_signal)() {
            return Err(Error::Interrupted(signal));
        }
        self.client
            .get_json(&format!("{}{path}", self.base_url), query)
            .map_err(Error::from_http)
    }
}

impl ModrinthApi for HttpModrinthApi {
    fn search(&self, query: &str, facets: &str, limit: u32) -> Result<Vec<SearchHit>, Error> {
        let limit = limit.to_string();
        let results: SearchResults = self.get(
            "/search",
            &[("query", query), ("facets", facets), ("limit", &limit)],
        )?;
        Ok(results.hits)
    }

    fn project(&self, identifier: &str) -> Result<Project, Error> {
        // Of the identifier characters, only these two would change the URL's path.
        let path_element = identifier.replace('"', "%22").replace('`', "%60");
        self.get(&format!("/project/{path_element}"), &[])
            .map_err(|error| match error {
                Error::Http(
                    source @ http_client::Error::Status {
                        status: NOT_FOUND, ..
                    },
                ) => Error::ProjectNotFound {
                    identifier: identifier.to_owned(),
                    source,
                },
                error => error,
            })
    }

    fn project_versions(&self, project_id: &str) -> Result<Vec<Version>, Error> {
        self.get(
            &format!("/project/{project_id}/version"),
            &[("include_changelog", "false")],
        )
    }

    fn projects(&self, ids: &BTreeSet<String>) -> Result<Vec<ProjectSlug>, Error> {
        self.get("/projects", &[("ids", &json_array(ids))])
    }

    fn versions(&self, ids: &BTreeSet<String>) -> Result<Vec<VersionProject>, Error> {
        self.get("/versions", &[("ids", &json_array(ids))])
    }
}

/// The ids, each once and in ascending order, as a JSON array (e.g. `["3wmN97b8"]`).
fn json_array(ids: &BTreeSet<String>) -> String {
    serde_json::to_string(ids).expect("a set of strings is representable in JSON")
}

/// The project whose slug is exactly `identifier`, case included (plg-3, dp-4).
pub fn identified_project(api: &dyn ModrinthApi, identifier: &str) -> Result<Project, Error> {
    let project = api.project(identifier)?;
    if project.slug == identifier {
        Ok(project)
    } else {
        Err(Error::NotTheIdentifier {
            given: identifier.to_owned(),
            slug: project.slug,
        })
    }
}

/// The identifiers of the required dependencies of each of `versions`, with at most one request
/// to the endpoint for several versions and one to the endpoint for several projects.
pub fn required_dependency_identifiers(
    api: &dyn ModrinthApi,
    versions: &[&Version],
) -> Result<Vec<BTreeSet<String>>, Error> {
    let required: Vec<Vec<&Dependency>> = versions
        .iter()
        .map(|version| {
            version
                .dependencies
                .iter()
                .filter(|dependency| dependency.is_required())
                .collect()
        })
        .collect();
    let project_of_version = projects_of_versions(api, required.iter().flatten().copied())?;
    let mut project_ids = Vec::with_capacity(required.len());
    for dependencies in &required {
        let mut ids = BTreeSet::new();
        for dependency in dependencies {
            ids.extend(project_id(dependency, &project_of_version)?);
        }
        project_ids.push(ids);
    }
    let slugs = slugs_of_projects(api, project_ids.iter().flatten().cloned().collect())?;
    Ok(project_ids
        .into_iter()
        .map(|ids| {
            ids.iter()
                .map(|id| {
                    slugs
                        .get(id)
                        .cloned()
                        .expect("`slugs_of_projects` returns a slug for every id")
                })
                .collect()
        })
        .collect())
}

/// Step 2: the project of each version named by a dependency without a project.
fn projects_of_versions<'a>(
    api: &dyn ModrinthApi,
    dependencies: impl Iterator<Item = &'a Dependency>,
) -> Result<BTreeMap<String, String>, Error> {
    let version_ids: BTreeSet<String> = dependencies
        .filter(|dependency| dependency.project_id.is_none())
        .filter_map(|dependency| dependency.version_id.clone())
        .collect();
    if version_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    Ok(api
        .versions(&version_ids)?
        .into_iter()
        .map(|version| (version.id, version.project_id))
        .collect())
}

/// Steps 1 to 3: the project a dependency names, none for one without project and version.
fn project_id(
    dependency: &Dependency,
    project_of_version: &BTreeMap<String, String>,
) -> Result<Option<String>, Error> {
    match (&dependency.project_id, &dependency.version_id) {
        (Some(project_id), _) => Ok(Some(project_id.clone())),
        (None, Some(version_id)) => project_of_version
            .get(version_id)
            .cloned()
            .map(Some)
            .ok_or_else(|| Error::DependencyVersionNotReturned {
                id: version_id.clone(),
            }),
        (None, None) => Ok(None),
    }
}

/// Step 4: the slug of each project; every project must be in the response.
fn slugs_of_projects(
    api: &dyn ModrinthApi,
    project_ids: BTreeSet<String>,
) -> Result<BTreeMap<String, String>, Error> {
    if project_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let slugs: BTreeMap<String, String> = api
        .projects(&project_ids)?
        .into_iter()
        .map(|project| (project.id, project.slug))
        .collect();
    match project_ids.into_iter().find(|id| !slugs.contains_key(id)) {
        Some(id) => Err(Error::DependencyProjectNotReturned { id }),
        None => Ok(slugs),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tokio::runtime::Runtime;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const LUCKPERMS_PROJECT: &str =
        include_str!("../../tests/fixtures/modrinth-project-luckperms.json");
    const PLUGIN_FACETS: &str = r#"[["project_type:plugin"]]"#;

    /// Answers every request with one response, and records the requests.
    struct FakeServer {
        runtime: Runtime,
        server: MockServer,
    }

    impl FakeServer {
        fn responding(status: u16, body: &str) -> FakeServer {
            let runtime = Runtime::new().unwrap();
            let server = runtime.block_on(async {
                let server = MockServer::start().await;
                Mock::given(method("GET"))
                    .respond_with(
                        ResponseTemplate::new(status)
                            .set_body_raw(body.as_bytes().to_vec(), "application/json"),
                    )
                    .mount(&server)
                    .await;
                server
            });
            FakeServer { runtime, server }
        }

        fn api(&self) -> HttpModrinthApi {
            HttpModrinthApi {
                received_signal: || None,
                ..HttpModrinthApi::with_base_url(
                    HttpClient::new().unwrap(),
                    &format!("{}/v2/", self.server.uri()),
                )
            }
        }

        fn requests(&self) -> Vec<wiremock::Request> {
            self.runtime
                .block_on(self.server.received_requests())
                .unwrap()
        }

        /// The path and the decoded query parameters of the only request.
        fn only_request(&self) -> (String, Vec<(String, String)>) {
            let requests = self.requests();
            assert_eq!(requests.len(), 1);
            let url = &requests[0].url;
            let query = url
                .query_pairs()
                .map(|(name, value)| (name.into_owned(), value.into_owned()))
                .collect();
            (url.path().to_owned(), query)
        }
    }

    fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn ids(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    /// d-modrinth-1
    #[test]
    fn no_request_is_sent_after_a_signal() {
        let server = FakeServer::responding(200, r#"{"hits":[]}"#);
        let api = HttpModrinthApi {
            received_signal: || Some(Signal::Interrupt),
            ..server.api()
        };

        let _ = api.search("luckperms", PLUGIN_FACETS, 10);

        assert_eq!(server.requests().len(), 0);
    }

    /// d-modrinth-1: the request not sent is reported as the interruption.
    #[test]
    fn request_after_a_signal_is_an_interruption() {
        let server = FakeServer::responding(200, r#"{"hits":[]}"#);
        let api = HttpModrinthApi {
            received_signal: || Some(Signal::Terminate),
            ..server.api()
        };

        let error = api.search("luckperms", PLUGIN_FACETS, 10).unwrap_err();

        assert_eq!(error.interrupted_by(), Some(Signal::Terminate));
    }

    /// d-modrinth-4
    #[test]
    fn too_many_requests_hints_at_waiting_a_minute() {
        let server = FakeServer::responding(
            429,
            r#"{"error":"ratelimit_error","description":"You are being rate-limited."}"#,
        );

        let error = server
            .api()
            .search("luckperms", PLUGIN_FACETS, 10)
            .unwrap_err();

        assert_eq!(
            error.hint().as_deref(),
            Some("wait a minute and run the command again")
        );
    }

    /// srch-4: another failed request keeps the hint to check the network.
    #[test]
    fn server_error_hints_at_the_network() {
        let server = FakeServer::responding(500, "");

        let error = server.api().project_versions("Vebnzrzj").unwrap_err();

        assert_eq!(
            error.hint().as_deref(),
            Some("check your network connection")
        );
    }

    /// 「Modrinth の API」 方法: the search sends the term, the facet it is given, and the limit.
    #[test]
    fn search_sends_the_term_the_facet_and_the_limit() {
        let server = FakeServer::responding(200, r#"{"hits":[]}"#);

        server
            .api()
            .search("multiverse portals", PLUGIN_FACETS, 10)
            .unwrap();

        assert_eq!(
            server.only_request(),
            (
                "/v2/search".to_owned(),
                pairs(&[
                    ("query", "multiverse portals"),
                    ("facets", PLUGIN_FACETS),
                    ("limit", "10")
                ])
            )
        );
    }

    /// 「Modrinth の API」 方法: the search sends the datapack facet it is given (dsrch-1).
    #[test]
    fn datapack_search_sends_the_datapack_facet() {
        let server = FakeServer::responding(200, r#"{"hits":[]}"#);

        server
            .api()
            .search("true ending", r#"[["project_type:datapack"]]"#, 10)
            .unwrap();

        assert_eq!(
            server.only_request().1,
            pairs(&[
                ("query", "true ending"),
                ("facets", r#"[["project_type:datapack"]]"#),
                ("limit", "10")
            ])
        );
    }

    /// 「Modrinth の API」 方法: a project that does not exist is told apart, with the identifier.
    #[test]
    fn missing_project_is_reported_with_its_identifier() {
        let server = FakeServer::responding(404, "");

        let error = server.api().project("nosuchplugin").unwrap_err();

        assert!(
            matches!(&error, Error::ProjectNotFound { identifier, .. } if identifier == "nosuchplugin"),
            "{error}"
        );
    }

    /// 「Modrinth の API」 方法: `"` and `` ` `` are escaped in the project path.
    #[test]
    fn project_path_escapes_quote_and_backtick() {
        let server = FakeServer::responding(200, LUCKPERMS_PROJECT);

        server.api().project("a\"b`c").unwrap();

        assert_eq!(server.only_request().0, "/v2/project/a%22b%60c");
    }

    /// 「Modrinth の API」 方法: the version list is requested by project id without changelogs.
    #[test]
    fn version_list_is_requested_without_changelogs() {
        let server = FakeServer::responding(200, "[]");

        server.api().project_versions("8VMk6P0I").unwrap();

        assert_eq!(
            server.only_request(),
            (
                "/v2/project/8VMk6P0I/version".to_owned(),
                pairs(&[("include_changelog", "false")])
            )
        );
    }

    /// 「Modrinth の API」 方法: several projects are requested with the ids as a JSON array.
    #[test]
    fn several_projects_are_requested_with_a_json_array() {
        let server = FakeServer::responding(200, "[]");

        server.api().projects(&ids(&["b", "a"])).unwrap();

        assert_eq!(
            server.only_request(),
            ("/v2/projects".to_owned(), pairs(&[("ids", r#"["a","b"]"#)]))
        );
    }

    /// 「Modrinth の API」 方法: several versions are requested with the ids as a JSON array.
    #[test]
    fn several_versions_are_requested_with_a_json_array() {
        let server = FakeServer::responding(200, "[]");

        server.api().versions(&ids(&["x"])).unwrap();

        assert_eq!(
            server.only_request(),
            ("/v2/versions".to_owned(), pairs(&[("ids", r#"["x"]"#)]))
        );
    }

    /// Returns fixed answers and records the endpoints called.
    #[derive(Default)]
    struct FakeApi {
        project: Option<Project>,
        projects: Vec<ProjectSlug>,
        versions: Vec<VersionProject>,
        calls: RefCell<Vec<String>>,
    }

    impl ModrinthApi for FakeApi {
        fn search(
            &self,
            _query: &str,
            _facets: &str,
            _limit: u32,
        ) -> Result<Vec<SearchHit>, Error> {
            unreachable!("not used by these tests")
        }

        fn project(&self, identifier: &str) -> Result<Project, Error> {
            self.calls
                .borrow_mut()
                .push(format!("project {identifier}"));
            Ok(self.project.clone().unwrap())
        }

        fn project_versions(&self, _project_id: &str) -> Result<Vec<Version>, Error> {
            unreachable!("not used by these tests")
        }

        fn projects(&self, ids: &BTreeSet<String>) -> Result<Vec<ProjectSlug>, Error> {
            self.calls.borrow_mut().push(format!("projects {ids:?}"));
            Ok(self
                .projects
                .iter()
                .filter(|project| ids.contains(&project.id))
                .cloned()
                .collect())
        }

        fn versions(&self, ids: &BTreeSet<String>) -> Result<Vec<VersionProject>, Error> {
            self.calls.borrow_mut().push(format!("versions {ids:?}"));
            Ok(self
                .versions
                .iter()
                .filter(|version| ids.contains(&version.id))
                .cloned()
                .collect())
        }
    }

    fn multiverse_core() -> ProjectSlug {
        ProjectSlug {
            id: "3wmN97b8".to_owned(),
            slug: "multiverse-core".to_owned(),
        }
    }

    fn dependency(project_id: Option<&str>, version_id: Option<&str>, kind: &str) -> Dependency {
        Dependency {
            project_id: project_id.map(str::to_owned),
            version_id: version_id.map(str::to_owned),
            dependency_type: kind.to_owned(),
        }
    }

    fn version_with(dependencies: Vec<Dependency>) -> Version {
        Version {
            id: "v".to_owned(),
            version_number: "1.0".to_owned(),
            version_type: "release".to_owned(),
            loaders: vec!["paper".to_owned()],
            game_versions: vec!["26.3".to_owned()],
            date_published: "2026-10-21T00:00:00Z".parse().unwrap(),
            dependencies,
            files: Vec::new(),
        }
    }

    /// vers-2: a required dependency's project id gives its slug.
    #[test]
    fn required_dependency_with_a_project_id_is_its_slug() {
        let api = FakeApi {
            projects: vec![multiverse_core()],
            ..FakeApi::default()
        };
        let version = version_with(vec![dependency(Some("3wmN97b8"), None, "required")]);

        let identifiers = required_dependency_identifiers(&api, &[&version]).unwrap();

        assert_eq!(identifiers, [ids(&["multiverse-core"])]);
    }

    /// d-modrinth-2
    #[test]
    fn required_dependency_with_only_a_version_id_is_the_slug_of_its_project() {
        let api = FakeApi {
            projects: vec![multiverse_core()],
            versions: vec![VersionProject {
                id: "sy2ZlvlF".to_owned(),
                project_id: "3wmN97b8".to_owned(),
            }],
            ..FakeApi::default()
        };
        let version = version_with(vec![dependency(None, Some("sy2ZlvlF"), "required")]);

        let identifiers = required_dependency_identifiers(&api, &[&version]).unwrap();

        assert_eq!(identifiers, [ids(&["multiverse-core"])]);
    }

    /// d-modrinth-3
    #[test]
    fn required_dependency_without_project_and_version_is_left_out() {
        let api = FakeApi::default();
        let version = version_with(vec![dependency(None, None, "required")]);

        let identifiers = required_dependency_identifiers(&api, &[&version]).unwrap();

        assert_eq!(identifiers, [ids(&[])]);
    }

    /// 「Modrinth の API」 方法 2 and 4: without dependencies to resolve no endpoint is called.
    #[test]
    fn no_endpoint_is_called_without_required_dependencies() {
        let api = FakeApi::default();
        let version = version_with(vec![dependency(Some("3wmN97b8"), None, "optional")]);

        required_dependency_identifiers(&api, &[&version]).unwrap();

        assert_eq!(api.calls.borrow().len(), 0);
    }

    /// 「Modrinth の API」 方法: the dependencies of all versions are resolved with one request
    /// to each endpoint.
    #[test]
    fn dependencies_of_all_versions_are_resolved_with_one_request_each() {
        let api = FakeApi {
            projects: vec![
                multiverse_core(),
                ProjectSlug {
                    id: "Vebnzrzj".to_owned(),
                    slug: "luckperms".to_owned(),
                },
            ],
            versions: vec![VersionProject {
                id: "sy2ZlvlF".to_owned(),
                project_id: "Vebnzrzj".to_owned(),
            }],
            ..FakeApi::default()
        };
        let first = version_with(vec![dependency(Some("3wmN97b8"), None, "required")]);
        let second = version_with(vec![dependency(None, Some("sy2ZlvlF"), "required")]);

        required_dependency_identifiers(&api, &[&first, &second]).unwrap();

        assert_eq!(
            *api.calls.borrow(),
            [
                r#"versions {"sy2ZlvlF"}"#,
                r#"projects {"3wmN97b8", "Vebnzrzj"}"#
            ]
        );
    }

    /// 「Modrinth の API」 方法 4: a project missing from the response is an error naming its id.
    #[test]
    fn project_missing_from_the_response_is_an_error() {
        let api = FakeApi::default();
        let version = version_with(vec![dependency(Some("3wmN97b8"), None, "required")]);

        let error = required_dependency_identifiers(&api, &[&version]).unwrap_err();

        assert!(error.to_string().contains("3wmN97b8"), "{error}");
    }

    /// plg-3: the slug that Modrinth returns must equal the identifier given.
    #[test]
    fn identifier_differing_in_case_is_not_the_identifier() {
        let api = FakeApi {
            project: Some(Project {
                id: "Vebnzrzj".to_owned(),
                slug: "luckperms".to_owned(),
                title: "LuckPerms".to_owned(),
            }),
            ..FakeApi::default()
        };

        let error = identified_project(&api, "LuckPerms").unwrap_err();

        assert_eq!(
            error.hint().as_deref(),
            Some("use the identifier in the source: `modrinth:luckperms`")
        );
    }

    /// plg-3: the project whose slug is the identifier is identified.
    #[test]
    fn identifier_equal_to_the_slug_identifies_the_project() {
        let project = Project {
            id: "Vebnzrzj".to_owned(),
            slug: "luckperms".to_owned(),
            title: "LuckPerms".to_owned(),
        };
        let api = FakeApi {
            project: Some(project.clone()),
            ..FakeApi::default()
        };

        assert_eq!(identified_project(&api, "luckperms").unwrap(), project);
    }
}
