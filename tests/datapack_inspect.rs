mod common;

use common::datapack_server_directory::DatapackEnvironment;
use common::fake_modrinth::{
    ServedFile, TRUE_ENDING_FILE_NAME, TRUE_ENDING_NAME, TRUE_ENDING_SOURCE, sha256_hex,
};
use common::management_environment::{Record, mism_toml_with_records};
use common::mism_process::Output;
use common::plugin_output::{described, reported_an_error};
use common::stdin_connection::StdinConnection;
use common::test_datapack_zip::{
    datapack_declaring, datapack_with_pack_mcmeta, not_a_zip_file, pack_version, server_jar,
    true_ending_pack_mcmeta, version_json, zip, zip_with_pack_mcmeta_in_a_directory,
};

const EXAMPLE_PATH: &str = "/downloads/Example.zip";
const STELLARITY_PATCH: &str = "stellarity_x_true_ending_patch";
const MECHANICS_PATCH_SOURCE: &str =
    "modrinth:stellarity_x_true_ending_patch:1.0.0:Stellarity_Mechanics_Patch.zip";
/// The Minecraft versions Modrinth states for the datapack version 1.1.4d of True Ending, in its
/// order.
const TRUE_ENDING_MINECRAFT_VERSIONS: &str =
    "1.21.2,1.21.3,1.21.4,1.21.5,1.21.6,1.21.7,1.21.8,1.21.9,1.21.10,1.21.11,26.1,26.1.1,26.1.2";

/// A datapack for exactly the datapack format 121, that of the test servers.
fn example_datapack() -> Vec<u8> {
    datapack_declaring(r#""min_format": 121, "max_format": 121"#)
}

fn true_ending_datapack() -> Vec<u8> {
    datapack_with_pack_mcmeta(&true_ending_pack_mcmeta())
}

/// `mism datapack inspect <local Example.zip>` of a file with `bytes`.
fn inspect_local(environment: &DatapackEnvironment, bytes: &[u8]) -> Output {
    let zip = environment.write_local_zip("Example.zip", bytes);
    environment
        .mism_datapack(["inspect", zip.to_str().unwrap()])
        .run()
}

/// `mism datapack inspect <local Example.zip>` without a target server, of a datapack that
/// declares `format_members`.
fn inspect_declaring(format_members: &str) -> Output {
    inspect_local(
        &DatapackEnvironment::without_server(),
        &datapack_declaring(format_members),
    )
}

/// `mism datapack inspect <local Example.zip>` on a Paper 26.3 server, whose datapack format is
/// 121.0, of a datapack that declares `format_members`.
fn inspect_on_the_server(format_members: &str) -> Output {
    inspect_local(
        &DatapackEnvironment::with_server("paper", "26.3"),
        &datapack_declaring(format_members),
    )
}

/// `mism datapack inspect <local Example.zip>` of a datapack that declares `format_members`, on
/// a Paper server for `minecraft_version` whose server jar is `jar`.
fn inspect_with_server_jar(minecraft_version: &str, jar: &[u8], format_members: &str) -> Output {
    let environment = DatapackEnvironment::with_server_without_jar("paper", minecraft_version);
    environment.put_server_jar("paper", minecraft_version, jar);
    inspect_local(&environment, &datapack_declaring(format_members))
}

/// `mism datapack inspect modrinth:true-ending:1.1.4d` in `environment`.
fn inspect_true_ending(environment: &DatapackEnvironment) -> Output {
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());
    environment
        .mism_datapack(["inspect", TRUE_ENDING_SOURCE])
        .run()
}

/// `mism datapack inspect` of the Stellarity patch's `Stellarity_Mechanics_Patch.zip` on a
/// Paper server for `minecraft_version` with the datapack records `datapacks` and the plugin
/// records `plugins`.
fn inspect_stellarity_patch(
    minecraft_version: &str,
    plugins: &[Record],
    datapacks: &[Record],
) -> Output {
    let environment = DatapackEnvironment::with_server("paper", minecraft_version);
    environment.write_mism_toml(&mism_toml_with_records(
        "paper",
        minecraft_version,
        plugins,
        datapacks,
    ));
    environment.modrinth.mount_project(STELLARITY_PATCH);
    environment.modrinth.mount_version_list(
        STELLARITY_PATCH,
        &[ServedFile::new(
            "Stellarity_Mechanics_Patch.zip",
            &example_datapack(),
        )],
    );
    environment
        .modrinth
        .mount_projects("stellarity-true-ending", &["MCnBYP0b", "bZgeDzN8"]);
    environment
        .mism_datapack(["inspect", MECHANICS_PATCH_SOURCE])
        .run()
}

/// A record whose source is `source`.
fn record_from(source: &str) -> Record<'_> {
    Record {
        name: "Recorded",
        source,
        file_name: "Recorded.zip",
        sha256: "0000000000000000000000000000000000000000000000000000000000000000",
    }
}

/// dinsp-1: a source whose file is not in the cache directory is fetched first.
#[test]
fn uncached_source_is_fetched_first() {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());
    environment.mism_datapack(["inspect", &url, "--yes"]).run();

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: Example\t-\t{}\t{url}\n",
            sha256_hex(&example_datapack())
        )
    );
}

/// dinsp-1: the file in the cache directory is inspected: the name given when it was fetched
/// stays, as nothing is fetched or asked again.
#[test]
fn cached_file_is_inspected() {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());
    environment
        .mism_datapack(["fetch", &url])
        .stdin(StdinConnection::ClosedAfter(b"Renamed\n".to_vec()))
        .run();

    let output = environment.mism_datapack(["inspect", &url]).run();

    assert_eq!(described(&output.stdout, "name"), Some("name\tRenamed"));
}

/// dinsp-1: inspecting exits with status 0.
#[test]
fn inspection_exits_with_status_0() {
    let output = inspect_declaring(r#""min_format": 121, "max_format": 121"#);

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// d-inspect-1: a cached file is inspected without communication.
#[test]
fn cached_file_is_inspected_without_communication() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());
    environment.fetch(TRUE_ENDING_SOURCE);
    let requests_before = environment.modrinth.request_count();

    environment
        .mism_datapack(["inspect", TRUE_ENDING_SOURCE])
        .run();

    assert_eq!(environment.modrinth.request_count(), requests_before);
}

/// dinsp-2: the 9 values of a datapack fetched from a URL, in order.
#[test]
fn values_of_a_url_datapack() {
    let environment = DatapackEnvironment::without_server();
    let url = environment
        .modrinth
        .mount_jar(EXAMPLE_PATH, &example_datapack());

    let output = environment.mism_datapack(["inspect", &url, "--yes"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: datapack name from the URL: Example\n\
             mism: name\tExample\n\
             mism: version\t-\n\
             mism: source\t{url}\n\
             mism: file.name\tExample.zip\n\
             mism: file.sha256\t{}\n\
             mism: declaration.min_format\t121\n\
             mism: declaration.max_format\t121\n\
             mism: modrinth.minecraft_versions\tunknown\n\
             mism: modrinth.required_dependencies\tunknown\n",
            sha256_hex(&example_datapack())
        )
    );
}

/// dinsp-2: the 9 values of a datapack fetched from Modrinth, in order: the example of the
/// design.
#[test]
fn values_of_a_modrinth_datapack() {
    let output = inspect_true_ending(&DatapackEnvironment::without_server());

    assert_eq!(
        output.stdout,
        format!(
            "mism: name\t{TRUE_ENDING_NAME}\n\
             mism: version\t1.1.4d\n\
             mism: source\t{TRUE_ENDING_SOURCE}\n\
             mism: file.name\t{TRUE_ENDING_FILE_NAME}\n\
             mism: file.sha256\t{}\n\
             mism: declaration.min_format\t71\n\
             mism: declaration.max_format\t9999\n\
             mism: modrinth.minecraft_versions\t{TRUE_ENDING_MINECRAFT_VERSIONS}\n\
             mism: modrinth.required_dependencies\t-\n",
            sha256_hex(&true_ending_datapack())
        )
    );
}

/// dinsp-2: the required dependencies are the identifiers of the projects Modrinth states.
#[test]
fn required_dependencies_are_identifiers() {
    let output = inspect_stellarity_patch("26.1.2", &[], &[]);

    assert_eq!(
        described(&output.stdout, "modrinth.required_dependencies"),
        Some("modrinth.required_dependencies\tstellarity,true-ending")
    );
}

/// The lower and the upper end that mism printed (dinsp-2).
fn printed_formats(output: &Output) -> [Option<&str>; 2] {
    [
        described(&output.stdout, "declaration.min_format"),
        described(&output.stdout, "declaration.max_format"),
    ]
}

/// dinsp-2: `min_format` and `max_format` come before `supported_formats` and `pack_format`.
#[test]
fn min_and_max_format_are_the_ends() {
    let output = inspect_declaring(
        r#""min_format": 80, "max_format": 90, "supported_formats": [1, 2], "pack_format": 3"#,
    );

    assert_eq!(
        printed_formats(&output),
        [
            Some("declaration.min_format\t80"),
            Some("declaration.max_format\t90")
        ]
    );
}

/// dinsp-2: without `min_format` and `max_format`, an integer `supported_formats` is both ends,
/// before `pack_format`.
#[test]
fn integer_supported_formats_is_both_ends() {
    let output = inspect_declaring(r#""supported_formats": 48, "pack_format": 3"#);

    assert_eq!(
        printed_formats(&output),
        [
            Some("declaration.min_format\t48"),
            Some("declaration.max_format\t48")
        ]
    );
}

/// dinsp-2: an array `supported_formats` has the two ends.
#[test]
fn array_supported_formats_has_the_ends() {
    let output = inspect_declaring(r#""supported_formats": [48, 57], "pack_format": 3"#);

    assert_eq!(
        printed_formats(&output),
        [
            Some("declaration.min_format\t48"),
            Some("declaration.max_format\t57")
        ]
    );
}

/// d-dpzip-4: a mapping `supported_formats` has the ends `min_inclusive` and `max_inclusive`.
#[test]
fn mapping_supported_formats_has_the_inclusive_ends() {
    let output = inspect_declaring(
        r#""pack_format": 71, "supported_formats": {"min_inclusive": 71, "max_inclusive": 9999}"#,
    );

    assert_eq!(
        printed_formats(&output),
        [
            Some("declaration.min_format\t71"),
            Some("declaration.max_format\t9999")
        ]
    );
}

/// dinsp-2: `pack_format` alone is both ends.
#[test]
fn pack_format_alone_is_both_ends() {
    let output = inspect_declaring(r#""pack_format": 48"#);

    assert_eq!(
        printed_formats(&output),
        [
            Some("declaration.min_format\t48"),
            Some("declaration.max_format\t48")
        ]
    );
}

/// dinsp-2: a declaration without formats has unknown ends.
#[test]
fn declaration_without_formats_has_unknown_ends() {
    let output = inspect_declaring("");

    assert_eq!(
        printed_formats(&output),
        [
            Some("declaration.min_format\tunknown"),
            Some("declaration.max_format\tunknown")
        ]
    );
}

/// d-dpzip-3, dinsp-2: an array of two integers is printed with a dot, an integer as it is.
#[test]
fn array_format_is_printed_with_a_dot() {
    let output = inspect_declaring(r#""min_format": [121, 0], "max_format": 121"#);

    assert_eq!(
        printed_formats(&output),
        [
            Some("declaration.min_format\t121.0"),
            Some("declaration.max_format\t121")
        ]
    );
}

/// dinsp-2: a datapack not from Modrinth has unknown Modrinth values.
#[test]
fn local_datapack_has_unknown_modrinth_values() {
    let output = inspect_declaring(r#""min_format": 121, "max_format": 121"#);

    assert_eq!(
        [
            described(&output.stdout, "modrinth.minecraft_versions"),
            described(&output.stdout, "modrinth.required_dependencies"),
        ],
        [
            Some("modrinth.minecraft_versions\tunknown"),
            Some("modrinth.required_dependencies\tunknown")
        ]
    );
}

/// dinsp-3: `declared` is `match` when Modrinth lists the server's Minecraft version.
#[test]
fn declared_is_match() {
    let output = inspect_true_ending(&DatapackEnvironment::with_server("paper", "26.1.2"));

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tmatch")
    );
}

/// dinsp-3: `declared` is `mismatch` when Modrinth does not list the server's Minecraft version.
#[test]
fn declared_is_mismatch() {
    let output = inspect_true_ending(&DatapackEnvironment::with_server("paper", "26.3"));

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tmismatch")
    );
}

/// dinsp-3: `declared` is `unknown` for a datapack not from Modrinth.
#[test]
fn declared_is_unknown() {
    let output = inspect_on_the_server(r#""min_format": 121, "max_format": 121"#);

    assert_eq!(
        described(&output.stdout, "declared"),
        Some("declared\tunknown")
    );
}

/// dinsp-3, d-compat-8: `pack_format` is `match` when the server's format is between the ends:
/// the integer upper end 121 includes 121.0.
#[test]
fn pack_format_is_match() {
    let output = inspect_on_the_server(r#""min_format": 121, "max_format": 121"#);

    assert_eq!(
        described(&output.stdout, "pack_format"),
        Some("pack_format\tmatch")
    );
}

/// dinsp-3: `pack_format` is `mismatch` when the server's format is not between the ends.
#[test]
fn pack_format_is_mismatch() {
    let output = inspect_on_the_server(r#""pack_format": 48"#);

    assert_eq!(
        described(&output.stdout, "pack_format"),
        Some("pack_format\tmismatch")
    );
}

/// dinsp-3: `pack_format` is `unknown` when the declaration has no formats.
#[test]
fn pack_format_is_unknown() {
    let output = inspect_on_the_server("");

    assert_eq!(
        described(&output.stdout, "pack_format"),
        Some("pack_format\tunknown")
    );
}

/// d-compat-9
#[test]
fn lower_end_with_a_greater_minor_number_is_a_mismatch() {
    let output = inspect_on_the_server(r#""min_format": [121, 1], "max_format": 121"#);

    assert_eq!(
        described(&output.stdout, "pack_format"),
        Some("pack_format\tmismatch")
    );
}

/// d-compat-10
#[test]
fn integer_upper_end_includes_every_minor_number() {
    let output = inspect_with_server_jar(
        "1.21.9",
        &server_jar(&version_json("1.21.9", &pack_version(120, 5))),
        r#""min_format": 100, "max_format": 120"#,
    );

    assert_eq!(
        described(&output.stdout, "pack_format"),
        Some("pack_format\tmatch")
    );
}

/// d-sjar-1: the server's format is 121.0, the only format in the range from `[121, 0]` to
/// `[121, 0]`.
#[test]
fn server_jar_of_paper_26_3_has_the_format_121_0() {
    let output = inspect_on_the_server(r#""min_format": [121, 0], "max_format": [121, 0]"#);

    assert_eq!(
        described(&output.stdout, "pack_format"),
        Some("pack_format\tmatch")
    );
}

/// 「サーバーのデータパックの形式の番号」 方法 2: a `pack_version` with the integer `data` has the
/// format `<data>.0`.
#[test]
fn server_jar_with_data_has_its_major_number() {
    let output = inspect_with_server_jar(
        "1.21.4",
        &server_jar(&version_json("1.21.4", r#"{"resource": 46, "data": 61}"#)),
        r#""min_format": [61, 0], "max_format": [61, 0]"#,
    );

    assert_eq!(
        described(&output.stdout, "pack_format"),
        Some("pack_format\tmatch")
    );
}

/// 「サーバーのデータパックの形式の番号」 方法: a Folia server's jar is
/// `versions/<Minecraft version>/folia-<Minecraft version>.jar`.
#[test]
fn folia_server_jar_is_read() {
    let output = inspect_local(
        &DatapackEnvironment::with_server("folia", "26.3"),
        &example_datapack(),
    );

    assert_eq!(
        described(&output.stdout, "pack_format"),
        Some("pack_format\tmatch")
    );
}

/// dinsp-3: a required dependency without a record is `missing`.
#[test]
fn dependencies_without_records_are_missing() {
    let output = inspect_stellarity_patch("26.1.2", &[], &[]);

    assert_eq!(
        described(&output.stdout, "missing"),
        Some("missing\tstellarity,true-ending")
    );
}

/// dinsp-3: a required dependency with a datapack record of its identifier is not missing.
#[test]
fn dependency_with_a_datapack_record_is_not_missing() {
    let output =
        inspect_stellarity_patch("26.1.2", &[], &[record_from("modrinth:true-ending:1.1.4d")]);

    assert_eq!(
        described(&output.stdout, "missing"),
        Some("missing\tstellarity")
    );
}

/// d-compat-7
#[test]
fn dependency_with_a_plugin_record_is_not_missing() {
    let output =
        inspect_stellarity_patch("26.1.2", &[record_from("modrinth:stellarity:2.0.0")], &[]);

    assert_eq!(
        described(&output.stdout, "missing"),
        Some("missing\ttrue-ending")
    );
}

/// dinsp-3: with records of every required dependency, there is no `missing`.
#[test]
fn dependencies_with_records_are_not_missing() {
    let output = inspect_stellarity_patch(
        "26.1.2",
        &[record_from("modrinth:stellarity:2.0.0")],
        &[record_from("modrinth:true-ending")],
    );

    assert!(
        output
            .stdout
            .ends_with("\nmism: declared\tmatch\nmism: pack_format\tmatch\n"),
        "stdout: {}",
        output.stdout
    );
}

/// dinsp-3: the judgements follow the 9 values, in the order `declared`, `pack_format`,
/// `missing`.
#[test]
fn judgements_follow_the_values_in_order() {
    let output = inspect_stellarity_patch("26.3", &[], &[]);

    assert!(
        output.stdout.ends_with(
            "\nmism: modrinth.required_dependencies\tstellarity,true-ending\n\
             mism: declared\tmismatch\n\
             mism: pack_format\tmatch\n\
             mism: missing\tstellarity,true-ending\n"
        ),
        "stdout: {}",
        output.stdout
    );
}

/// dinsp-4: a file that is not a zip file is an error.
#[test]
fn file_that_is_not_zip_is_an_error() {
    let output = inspect_local(&DatapackEnvironment::without_server(), &not_a_zip_file());

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-4: a zip file without a top-level `pack.mcmeta` is an error, also when a directory in
/// it has one.
#[test]
fn zip_without_a_top_level_pack_mcmeta_is_an_error() {
    let output = inspect_local(
        &DatapackEnvironment::without_server(),
        &zip_with_pack_mcmeta_in_a_directory(),
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-4: a zip file without any `pack.mcmeta` is an error.
#[test]
fn zip_without_pack_mcmeta_is_an_error() {
    let output = inspect_local(
        &DatapackEnvironment::without_server(),
        &zip(&[("data/example/function/hello.mcfunction", "say hello\n")]),
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-4: `min_format` without `max_format` is an error.
#[test]
fn min_format_alone_is_an_error() {
    let output = inspect_declaring(r#""min_format": 121, "pack_format": 121"#);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-4: `max_format` without `min_format` is an error.
#[test]
fn max_format_alone_is_an_error() {
    let output = inspect_declaring(r#""max_format": 121, "pack_format": 121"#);

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-5: a target server without its server jar is an error.
#[test]
fn server_without_its_jar_is_an_error() {
    let output = inspect_local(
        &DatapackEnvironment::with_server_without_jar("paper", "26.3"),
        &example_datapack(),
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-5: a server jar that is not a zip file is an error.
#[test]
fn server_jar_that_is_not_zip_is_an_error() {
    let output = inspect_with_server_jar(
        "26.3",
        &not_a_zip_file(),
        r#""min_format": 121, "max_format": 121"#,
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-5: a server jar without `version.json` is an error.
#[test]
fn server_jar_without_version_json_is_an_error() {
    let output = inspect_with_server_jar(
        "26.3",
        &zip(&[("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\n")]),
        r#""min_format": 121, "max_format": 121"#,
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-5 (「サーバーのデータパックの形式の番号」 方法): a `pack_version` that is one integer is
/// an error.
#[test]
fn server_jar_with_an_integer_pack_version_is_an_error() {
    let output = inspect_with_server_jar(
        "1.16.5",
        &server_jar(&version_json("1.16.5", "6")),
        r#""pack_format": 6"#,
    );

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}

/// dinsp-5: the server jar of another Minecraft version than the server's is not read.
#[test]
fn server_jar_of_another_minecraft_version_is_an_error() {
    let environment = DatapackEnvironment::with_server_without_jar("paper", "26.3");
    environment.put_server_jar(
        "paper",
        "26.2",
        &server_jar(&version_json("26.2", &pack_version(121, 0))),
    );

    let output = inspect_local(&environment, &example_datapack());

    assert!(reported_an_error(&output), "stdout: {}", output.stdout);
}
