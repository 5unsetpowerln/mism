mod common;

use common::fake_modrinth::{
    LUCKPERMS_FILE_NAME, LUCKPERMS_SOURCE, ServedFile, saved_version_list, sha256_hex,
};
use common::mism_process::Output;
use common::plugin_output::{described, has_line};
use common::plugin_server_directory::{PluginEnvironment, Record, mism_toml_text};
use common::stdin_connection::StdinConnection;
use common::test_plugin_jar::{
    jar, jar_with_paper_plugin_yml, jar_with_plugin_yml, jar_without_declaration, plugin_yml,
};

const EXAMPLE_PATH: &str = "/downloads/Example.jar";
const PORTALS_SOURCE: &str = "modrinth:multiverse-portals:5.3.0";

/// The `plugin.yml` of the plugin `Example` with `extra` lines after the required keys.
fn example_yml(extra: &str) -> String {
    format!("{}{extra}", plugin_yml("Example", "1.0", "1.21"))
}

fn luckperms_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("LuckPerms", "5.5.71", "1.13"))
}

/// `mism plugin inspect <local Example.jar>` of a jar with `bytes`.
fn inspect_local(environment: &PluginEnvironment, bytes: &[u8]) -> Output {
    let jar = environment.write_local_jar("Example.jar", bytes);
    environment
        .mism_plugin(["inspect".as_ref(), jar.as_os_str()])
        .run()
}

/// `mism plugin inspect <local Example.jar>` without a target server, of a jar whose
/// `plugin.yml` is `plugin_yml`.
fn inspect_declaring(plugin_yml: &str) -> Output {
    inspect_local(
        &PluginEnvironment::without_server(),
        &jar_with_plugin_yml(plugin_yml),
    )
}

/// `mism plugin inspect <local Example.jar>` on a server of `software` for Minecraft 26.3, of a
/// jar whose `plugin.yml` is `plugin_yml`.
fn inspect_on(software: &str, plugin_yml: &str) -> Output {
    inspect_local(
        &PluginEnvironment::with_server(software, "26.3"),
        &jar_with_plugin_yml(plugin_yml),
    )
}

/// An environment whose fake Modrinth serves Multiverse-Portals 5.3.0 as `bytes`, and the
/// project of its required dependency Multiverse-Core.
fn serving_multiverse_portals(environment: PluginEnvironment, bytes: &[u8]) -> PluginEnvironment {
    environment.modrinth.mount_project("multiverse-portals");
    environment.modrinth.mount_version_list(
        "multiverse-portals",
        &[ServedFile::new("multiverse-portals-5.3.0.jar", bytes)],
    );
    environment.modrinth.mount_projects_multiverse_core();
    environment
}

/// `mism plugin inspect modrinth:multiverse-portals:5.3.0` in `environment`, of a jar whose
/// `plugin.yml` is `plugin_yml`.
fn inspect_multiverse_portals(environment: PluginEnvironment, plugin_yml: &str) -> Output {
    let environment = serving_multiverse_portals(environment, &jar_with_plugin_yml(plugin_yml));
    environment.mism_plugin(["inspect", PORTALS_SOURCE]).run()
}

/// insp-1: a source whose jar is not in the cache directory is fetched first.
#[test]
fn uncached_source_is_fetched_first() {
    let environment = PluginEnvironment::without_server();
    let bytes = jar_with_plugin_yml(&example_yml(""));
    let url = environment.modrinth.mount_jar(EXAMPLE_PATH, &bytes);
    environment.mism_plugin(["inspect", &url, "--yes"]).run();

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!("mism: Example\t-\t{}\t{url}\n", sha256_hex(&bytes))
    );
}

/// insp-1: the jar in the cache directory is inspected: the name given when it was fetched
/// stays, as nothing is fetched or asked again.
#[test]
fn cached_jar_is_inspected() {
    let environment = PluginEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &jar_with_plugin_yml(&example_yml("")));
    environment
        .mism_plugin(["fetch", &url])
        .stdin(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()))
        .run();

    let output = environment.mism_plugin(["inspect", &url]).run();

    assert_eq!(described(&output.stdout, "name"), Some("name\tRenamed"));
}

/// insp-1: inspecting exits with status 0.
#[test]
fn inspection_exits_with_status_0() {
    let output = inspect_declaring(&example_yml(""));

    assert_eq!(output.exit_code, 0);
}

/// insp-2: the 13 values of a jar fetched from a URL, in order.
#[test]
fn values_of_a_url_jar() {
    let environment = PluginEnvironment::without_server();
    let bytes = jar_with_plugin_yml(&example_yml("folia-supported: true\ndepend: [Vault]\n"));
    let url = environment.modrinth.mount_jar(EXAMPLE_PATH, &bytes);

    let output = environment.mism_plugin(["inspect", &url, "--yes"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: plugin name from the URL: Example\n\
             mism: name\tExample\n\
             mism: version\t-\n\
             mism: source\t{url}\n\
             mism: file.name\tExample.jar\n\
             mism: file.sha256\t{}\n\
             mism: declaration.name\tExample\n\
             mism: declaration.version\t1.0\n\
             mism: compatibility.minimum_minecraft_version\t1.21\tjar\n\
             mism: compatibility.minecraft_versions\tunknown\t-\n\
             mism: compatibility.software\tfolia\tjar\n\
             mism: compatibility.folia_supported\ttrue\tjar\n\
             mism: compatibility.required_dependencies\tVault\tjar\n\
             mism: compatibility.optional_dependencies\t-\tjar\n",
            sha256_hex(&bytes)
        )
    );
}

/// The Minecraft versions Modrinth states for LuckPerms v5.5.71-bukkit, in its order.
fn luckperms_game_versions() -> String {
    let versions = saved_version_list("luckperms");
    let version = versions
        .as_array()
        .unwrap()
        .iter()
        .find(|version| version["version_number"] == "v5.5.71-bukkit")
        .unwrap();
    version["game_versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|game_version| game_version.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(",")
}

/// insp-2: the 13 values of a jar fetched from Modrinth, in order.
#[test]
fn values_of_a_modrinth_jar() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.mism_plugin(["inspect", LUCKPERMS_SOURCE]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: name\tLuckPerms\n\
             mism: version\tv5.5.71-bukkit\n\
             mism: source\t{LUCKPERMS_SOURCE}\n\
             mism: file.name\t{LUCKPERMS_FILE_NAME}\n\
             mism: file.sha256\t{}\n\
             mism: declaration.name\tLuckPerms\n\
             mism: declaration.version\t5.5.71\n\
             mism: compatibility.minimum_minecraft_version\t1.13\tjar\n\
             mism: compatibility.minecraft_versions\t{}\tmodrinth\n\
             mism: compatibility.software\tbukkit,folia,paper,spigot\tmodrinth\n\
             mism: compatibility.folia_supported\tfalse\tjar\n\
             mism: compatibility.required_dependencies\t-\tjar,modrinth\n\
             mism: compatibility.optional_dependencies\t-\tjar\n",
            sha256_hex(&luckperms_jar()),
            luckperms_game_versions()
        )
    );
}

/// insp-2: software that both the declaration and Modrinth state comes from both.
#[test]
fn software_from_the_declaration_and_modrinth_has_both_sources() {
    let environment = PluginEnvironment::without_server();
    environment
        .modrinth
        .serve_luckperms(&jar_with_plugin_yml(&format!(
            "{}folia-supported: true\n",
            plugin_yml("LuckPerms", "5.5.71", "1.13")
        )));

    let output = environment.mism_plugin(["inspect", LUCKPERMS_SOURCE]).run();

    assert_eq!(
        described(&output.stdout, "compatibility.software"),
        Some("compatibility.software\tbukkit,folia,paper,spigot\tjar,modrinth")
    );
}

/// insp-2: the Paper plugin format adds `paper` to the software.
#[test]
fn paper_plugin_format_adds_paper() {
    let output = inspect_local(
        &PluginEnvironment::without_server(),
        &jar_with_paper_plugin_yml("name: Example\nversion: '1.0'\nmain: example.Main\n"),
    );

    assert_eq!(
        described(&output.stdout, "compatibility.software"),
        Some("compatibility.software\tpaper\tjar")
    );
}

/// insp-2: without Folia support, the Paper plugin format, and Modrinth, the software is
/// unknown.
#[test]
fn software_without_any_statement_is_unknown() {
    let output = inspect_declaring(&example_yml(""));

    assert_eq!(
        described(&output.stdout, "compatibility.software"),
        Some("compatibility.software\tunknown\t-")
    );
}

/// insp-2: without `api-version` the minimum Minecraft version is unknown.
#[test]
fn minimum_minecraft_version_without_api_version_is_unknown() {
    let output = inspect_declaring("name: Example\nversion: '1.0'\nmain: example.Main\n");

    assert_eq!(
        described(&output.stdout, "compatibility.minimum_minecraft_version"),
        Some("compatibility.minimum_minecraft_version\tunknown\t-")
    );
}

/// insp-2: the optional dependencies are the declaration's `softdepend`.
#[test]
fn optional_dependencies_are_the_softdepend() {
    let output = inspect_declaring(&example_yml("softdepend: [Vault, PlaceholderAPI]\n"));

    assert_eq!(
        described(&output.stdout, "compatibility.optional_dependencies"),
        Some("compatibility.optional_dependencies\tPlaceholderAPI,Vault\tjar")
    );
}

/// insp-2: the required dependencies are the declaration's names and Modrinth's identifiers.
#[test]
fn required_dependencies_merge_the_declaration_and_modrinth() {
    let output = inspect_multiverse_portals(
        PluginEnvironment::without_server(),
        &format!(
            "{}depend: [Multiverse-Core]\n",
            plugin_yml("Multiverse-Portals", "5.3.0", "1.18")
        ),
    );

    assert_eq!(
        described(&output.stdout, "compatibility.required_dependencies"),
        Some("compatibility.required_dependencies\tMultiverse-Core,multiverse-core\tjar,modrinth")
    );
}

/// d-compat-1
#[test]
fn url_jar_without_depend_has_no_required_dependency_from_the_jar() {
    let environment = PluginEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &jar_with_plugin_yml(&example_yml("")));

    let output = environment.mism_plugin(["inspect", &url, "--yes"]).run();

    assert_eq!(
        described(&output.stdout, "compatibility.required_dependencies"),
        Some("compatibility.required_dependencies\t-\tjar")
    );
}

/// d-compat-2
#[test]
fn dependency_from_both_sources_appears_once() {
    let output = inspect_multiverse_portals(
        PluginEnvironment::without_server(),
        &format!(
            "{}depend: [multiverse-core]\n",
            plugin_yml("Multiverse-Portals", "5.3.0", "1.18")
        ),
    );

    assert_eq!(
        described(&output.stdout, "compatibility.required_dependencies"),
        Some("compatibility.required_dependencies\tmultiverse-core\tjar,modrinth")
    );
}

/// d-compat-3
#[test]
fn unparsable_api_version_with_a_target_server_is_an_error() {
    let output = inspect_on("paper", &plugin_yml("Example", "1.0", "foo"));

    assert!(
        output
            .stdout
            .contains("\nmism: the api-version \"foo\" that ")
    );
}

/// d-compat-4: the value is printed as it is.
#[test]
fn unparsable_api_version_without_a_target_server_is_printed() {
    let output = inspect_declaring(&plugin_yml("Example", "1.0", "foo"));

    assert_eq!(
        described(&output.stdout, "compatibility.minimum_minecraft_version"),
        Some("compatibility.minimum_minecraft_version\tfoo\tjar")
    );
}

/// d-compat-4: the inspection succeeds.
#[test]
fn unparsable_api_version_without_a_target_server_exits_with_status_0() {
    let output = inspect_declaring(&plugin_yml("Example", "1.0", "foo"));

    assert_eq!(output.exit_code, 0);
}

/// d-jar-1
#[test]
fn unquoted_numbers_are_printed_as_written() {
    let output =
        inspect_declaring("name: Example\nversion: 2.0\nmain: example.Main\napi-version: 1.20\n");

    assert_eq!(
        [
            described(&output.stdout, "declaration.version"),
            described(&output.stdout, "compatibility.minimum_minecraft_version"),
        ],
        [
            Some("declaration.version\t2.0"),
            Some("compatibility.minimum_minecraft_version\t1.20\tjar"),
        ]
    );
}

/// d-jar-2
#[test]
fn plugin_yml_is_read_when_both_declarations_exist() {
    let output = inspect_local(
        &PluginEnvironment::without_server(),
        &jar(&[
            (
                "plugin.yml",
                plugin_yml("FromPluginYml", "1.0", "1.21").as_str(),
            ),
            (
                "paper-plugin.yml",
                plugin_yml("FromPaperPluginYml", "1.0", "1.21").as_str(),
            ),
        ]),
    );

    assert_eq!(
        described(&output.stdout, "declaration.name"),
        Some("declaration.name\tFromPluginYml")
    );
}

/// `mism plugin inspect` of a jar whose `paper-plugin.yml` has `dependencies`.
fn inspect_paper_dependencies(dependencies: &str) -> Output {
    inspect_local(
        &PluginEnvironment::without_server(),
        &jar_with_paper_plugin_yml(&format!(
            "name: Example\nversion: '1.0'\nmain: example.Main\ndependencies:\n{dependencies}"
        )),
    )
}

/// d-jar-3
#[test]
fn paper_dependency_without_required_is_required() {
    let output = inspect_paper_dependencies(
        "  bootstrap:\n    A: {}\n  server:\n    B:\n      required: false\n",
    );

    assert_eq!(
        [
            described(&output.stdout, "compatibility.required_dependencies"),
            described(&output.stdout, "compatibility.optional_dependencies"),
        ],
        [
            Some("compatibility.required_dependencies\tA\tjar"),
            Some("compatibility.optional_dependencies\tB\tjar"),
        ]
    );
}

/// d-jar-4
#[test]
fn paper_dependency_required_in_one_section_is_only_required() {
    let output = inspect_paper_dependencies(
        "  bootstrap:\n    A:\n      required: false\n  server:\n    A:\n      required: true\n",
    );

    assert_eq!(
        [
            described(&output.stdout, "compatibility.required_dependencies"),
            described(&output.stdout, "compatibility.optional_dependencies"),
        ],
        [
            Some("compatibility.required_dependencies\tA\tjar"),
            Some("compatibility.optional_dependencies\t-\tjar"),
        ]
    );
}

/// d-jar-5
#[test]
fn depend_that_is_not_a_list_is_an_error() {
    let output = inspect_declaring(&example_yml("depend: Vault\n"));

    assert!(
        output
            .stdout
            .lines()
            .any(|line| line.starts_with("mism: plugin.yml in ")
                && line.ends_with(" is not a valid plugin declaration"))
    );
}

/// d-jar-6: a `plugin.yml` only in a directory of the jar is no declaration.
#[test]
fn plugin_yml_below_a_directory_is_no_declaration() {
    let output = inspect_local(
        &PluginEnvironment::without_server(),
        &jar(&[("META-INF/plugin.yml", example_yml("").as_str())]),
    );

    assert!(
        output
            .stdout
            .ends_with(" has neither plugin.yml nor paper-plugin.yml, so it declares no plugin\n")
    );
}

/// d-jar-7: a file that is not a zip file is not reported as a jar without a declaration.
#[test]
fn file_that_is_not_zip_is_a_distinct_error() {
    let output = inspect_local(&PluginEnvironment::without_server(), b"not a zip file");

    assert!(
        output
            .stdout
            .contains(" is not a zip file, so it is not a jar file\n")
    );
}

/// insp-3: `declared` is `match` when the software and the Minecraft version are listed.
#[test]
fn declared_is_match() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.mism_plugin(["inspect", LUCKPERMS_SOURCE]).run();

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tmatch")
    );
}

/// insp-3: `declared` is `software-only` when the server's Minecraft version is not listed.
#[test]
fn declared_is_software_only() {
    let environment = PluginEnvironment::with_server("paper", "26.4");
    environment.modrinth.serve_luckperms(&luckperms_jar());

    let output = environment.mism_plugin(["inspect", LUCKPERMS_SOURCE]).run();

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tsoftware-only")
    );
}

/// insp-3: `declared` is `mismatch` when the server's software is not listed.
#[test]
fn declared_is_mismatch() {
    let output = inspect_on("paper", &example_yml("folia-supported: true\n"));

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tmismatch")
    );
}

/// insp-3: `declared` is `unknown` when the software is unknown.
#[test]
fn declared_is_unknown() {
    let output = inspect_on("paper", &example_yml(""));

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tunknown")
    );
}

/// insp-3: a minimum Minecraft version newer than the server's is `incompatible`.
#[test]
fn newer_minimum_minecraft_version_is_incompatible() {
    let output = inspect_on("paper", &plugin_yml("Example", "1.0", "26.4"));

    assert_eq!(
        described(&output.stdout, "incompatible"),
        Some("incompatible\trequires Minecraft 26.4 or newer")
    );
}

/// insp-3: a minimum Minecraft version that is not newer is not `incompatible`.
#[test]
fn older_minimum_minecraft_version_is_not_incompatible() {
    let output = inspect_on("paper", &plugin_yml("Example", "1.0", "1.21"));

    assert_eq!(described(&output.stdout, "incompatible"), None);
}

/// insp-3: on Folia, a jar that does not declare Folia support is `incompatible`.
#[test]
fn folia_server_without_declared_support_is_incompatible() {
    let output = inspect_on("folia", &example_yml(""));

    assert_eq!(
        described(&output.stdout, "incompatible"),
        Some("incompatible\tdoes not declare Folia support")
    );
}

/// insp-3: a required dependency that the server lacks is `missing`.
#[test]
fn required_dependency_the_server_lacks_is_missing() {
    let output = inspect_on("paper", &example_yml("depend: [Vault]\n"));

    assert_eq!(described(&output.stdout, "missing"), Some("missing\tVault"));
}

/// insp-3: a required dependency declared by a jar in `plugins/` is not missing.
#[test]
fn dependency_declared_by_a_jar_in_plugins_is_not_missing() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.put_in_plugins(
        "Vault.jar",
        &jar_with_plugin_yml(&plugin_yml("Vault", "1.7", "1.13")),
    );

    let output = inspect_local(
        &environment,
        &jar_with_plugin_yml(&example_yml("depend: [Vault]\n")),
    );

    assert_eq!(described(&output.stdout, "missing"), None);
}

/// insp-3: a required dependency from Modrinth is missing without a record of its identifier.
#[test]
fn modrinth_dependency_without_a_record_is_missing() {
    let output = inspect_multiverse_portals(
        PluginEnvironment::with_server("paper", "26.3"),
        &plugin_yml("Multiverse-Portals", "5.3.0", "1.18"),
    );

    assert_eq!(
        described(&output.stdout, "missing"),
        Some("missing\tmultiverse-core")
    );
}

/// insp-3: a required dependency from Modrinth is present when a record's source has its
/// identifier.
#[test]
fn modrinth_dependency_with_a_record_is_not_missing() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.write_mism_toml(&mism_toml_text(
        "paper",
        "26.3",
        &[Record {
            name: "Multiverse-Core",
            source: "modrinth:multiverse-core:5.1.0",
            file_name: "multiverse-core-5.1.0.jar",
            sha256: &sha256_hex(b"multiverse-core"),
        }],
    ));

    let output = inspect_multiverse_portals(
        environment,
        &plugin_yml("Multiverse-Portals", "5.3.0", "1.18"),
    );

    assert_eq!(described(&output.stdout, "missing"), None);
}

/// insp-3: another jar in `plugins/` that declares the same name is a `conflict`.
#[test]
fn jar_declaring_the_same_name_is_a_conflict() {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.put_in_plugins("Old.jar", &jar_with_plugin_yml(&example_yml("")));

    let output = inspect_local(&environment, &jar_with_plugin_yml(&example_yml("")));

    assert_eq!(
        described(&output.stdout, "conflict"),
        Some("conflict\tOld.jar")
    );
}

/// insp-3: the judgements follow the 13 values, in the order `declared`, `incompatible`,
/// `missing`, `conflict`.
#[test]
fn judgements_follow_the_values_in_order() {
    let environment = PluginEnvironment::with_server("folia", "26.3");
    environment.put_in_plugins("Old.jar", &jar_with_plugin_yml(&example_yml("")));

    let output = inspect_local(
        &environment,
        &jar_with_plugin_yml(&format!(
            "{}depend: [Vault]\n",
            plugin_yml("Example", "1.0", "26.4")
        )),
    );

    assert!(output.stdout.ends_with(
        "\tjar\n\
         mism: declared\tunknown\n\
         mism: incompatible\tdoes not declare Folia support,requires Minecraft 26.4 or newer\n\
         mism: missing\tVault\n\
         mism: conflict\tOld.jar\n"
    ));
}

/// insp-4: a jar without a declaration is an error.
#[test]
fn jar_without_a_declaration_is_an_error() {
    let output = inspect_local(
        &PluginEnvironment::without_server(),
        &jar_without_declaration(),
    );

    assert!(
        output
            .stdout
            .ends_with(" has neither plugin.yml nor paper-plugin.yml, so it declares no plugin\n")
    );
}

/// insp-4: the error ends mism with status 1.
#[test]
fn jar_without_a_declaration_exits_with_status_1() {
    let output = inspect_local(
        &PluginEnvironment::without_server(),
        &jar_without_declaration(),
    );

    assert_eq!(output.exit_code, 1);
}

/// d-inspect-1
#[test]
fn cached_jar_is_inspected_without_communication() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());
    environment.fetch(LUCKPERMS_SOURCE);
    let requests_before = environment.modrinth.request_count();

    environment.mism_plugin(["inspect", LUCKPERMS_SOURCE]).run();

    assert_eq!(environment.modrinth.request_count(), requests_before);
}

/// insp-2: the first value names the plugin by its name as fetched (plg-5).
#[test]
fn name_is_the_plugin_name() {
    let output = inspect_declaring(&plugin_yml("Declared", "1.0", "1.21"));

    assert!(has_line(&output.stdout, "name\tExample"));
}
