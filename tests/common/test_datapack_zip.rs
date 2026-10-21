use super::test_plugin_jar::jar;

/// A small zip file with each entry's path and text, made for one test: a datapack, or the
/// server jar that Paper and Folia create below `versions/`.
pub fn zip(entries: &[(&str, &str)]) -> Vec<u8> {
    // A jar file is a zip file.
    jar(entries)
}

/// The text of a `pack.mcmeta` whose `pack` has a description and the members `format_members`
/// (e.g. `"min_format": 121, "max_format": 121`, or nothing).
pub fn pack_mcmeta(format_members: &str) -> String {
    let separator = if format_members.is_empty() { "" } else { ", " };
    format!(r#"{{"pack": {{"description": "Example"{separator}{format_members}}}}}"#)
}

/// The `pack.mcmeta` of True Ending 1.1.4d as downloaded on 2026-10-21, with its description and
/// overlays shortened: it has all four ways of declaring the formats.
pub fn true_ending_pack_mcmeta() -> String {
    r##"{
  "pack": {
    "pack_format": 71,
    "min_format": 71,
    "max_format": 9999,
    "supported_formats": {
      "min_inclusive": 71,
      "max_inclusive": 9999
    },
    "description": [
      {"translate": "True Ending", "color": "light_purple"},
      {"translate": " [v1.1.4d]", "color": "#707070"}
    ]
  },
  "overlays": {"entries": []}
}
"##
    .to_owned()
}

/// A datapack whose top-level `pack.mcmeta` is `pack_mcmeta`, with one function.
pub fn datapack_with_pack_mcmeta(pack_mcmeta: &str) -> Vec<u8> {
    zip(&[
        ("pack.mcmeta", pack_mcmeta),
        ("data/example/function/hello.mcfunction", "say hello\n"),
    ])
}

/// A datapack whose top-level `pack.mcmeta` declares `format_members` (see `pack_mcmeta`).
pub fn datapack_declaring(format_members: &str) -> Vec<u8> {
    datapack_with_pack_mcmeta(&pack_mcmeta(format_members))
}

/// A zip file that has `pack.mcmeta` only inside the directory `inner/`, as when a datapack's
/// directory itself was zipped (dinsp-4).
pub fn zip_with_pack_mcmeta_in_a_directory() -> Vec<u8> {
    zip(&[
        (
            "inner/pack.mcmeta",
            &pack_mcmeta(r#""min_format": 121, "max_format": 121"#),
        ),
        (
            "inner/data/example/function/hello.mcfunction",
            "say hello\n",
        ),
    ])
}

/// A file that is not a zip file (dinsp-4).
pub fn not_a_zip_file() -> Vec<u8> {
    b"not a zip file\n".to_vec()
}

/// A `pack_version` of the shape of Paper 26.3's, whose datapack format is
/// `<data_major>.<data_minor>`.
pub fn pack_version(data_major: u32, data_minor: u32) -> String {
    format!(
        r#"{{
        "resource_major": 97,
        "resource_minor": 1,
        "data_major": {data_major},
        "data_minor": {data_minor}
    }}"#
    )
}

/// A `version.json` of the shape of Paper 26.3's, for `minecraft_version`, whose `pack_version`
/// is the JSON text `pack_version`. The other values are those in
/// `versions/26.3/paper-26.3.jar` of Paper 26.3 (version 209).
pub fn version_json(minecraft_version: &str, pack_version: &str) -> String {
    format!(
        r#"{{
    "id": "{minecraft_version}",
    "name": "{minecraft_version}",
    "world_version": 5023,
    "series_id": "main",
    "protocol_version": 777,
    "pack_version": {pack_version},
    "build_time": "2026-09-15T11:20:48+00:00",
    "java_component": "java-runtime-epsilon",
    "java_version": 25,
    "stable": true,
    "use_editor": false
}}"#
    )
}

/// A server jar below `versions/` with the top-level `version.json` `version_json`.
pub fn server_jar(version_json: &str) -> Vec<u8> {
    zip(&[
        ("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\n"),
        ("version.json", version_json),
    ])
}
