use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;

/// A small jar (zip) file with each entry's path and text, made for one test.
pub fn jar(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, text) in entries {
        writer
            .start_file(*path, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A jar whose only declaration is `plugin.yml` with the text `plugin_yml`.
pub fn jar_with_plugin_yml(plugin_yml: &str) -> Vec<u8> {
    jar(&[("plugin.yml", plugin_yml)])
}

/// A jar whose only declaration is `paper-plugin.yml` with the text `paper_plugin_yml`.
pub fn jar_with_paper_plugin_yml(paper_plugin_yml: &str) -> Vec<u8> {
    jar(&[("paper-plugin.yml", paper_plugin_yml)])
}

/// A jar without `plugin.yml` and `paper-plugin.yml` (insp-4).
pub fn jar_without_declaration() -> Vec<u8> {
    jar(&[("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\n")])
}

/// The text of a `plugin.yml` with the keys a server requires.
pub fn plugin_yml(name: &str, version: &str, api_version: &str) -> String {
    format!(
        "name: {name}\nversion: '{version}'\nmain: example.Main\napi-version: '{api_version}'\n"
    )
}
