mod common;

use std::fs;
use std::os::unix::fs::symlink;

use common::datapack_server_directory::DatapackEnvironment;
use common::fake_modrinth::{
    TRUE_ENDING_FILE_NAME, TRUE_ENDING_NAME, TRUE_ENDING_SOURCE, sha256_hex,
};
use common::management_environment::{Record, mism_toml_with_records};
use common::mism_process::Output;
use common::test_datapack_zip::{datapack_with_pack_mcmeta, true_ending_pack_mcmeta};

fn true_ending_datapack() -> Vec<u8> {
    datapack_with_pack_mcmeta(&true_ending_pack_mcmeta())
}

/// The record of True Ending 1.1.4d with the SHA-256 `sha256`.
fn true_ending_record(sha256: &str) -> Record<'_> {
    Record {
        name: TRUE_ENDING_NAME,
        source: TRUE_ENDING_SOURCE,
        file_name: TRUE_ENDING_FILE_NAME,
        sha256,
    }
}

/// `mism datapack list` in `environment` with the datapack records `records`, after `prepare`.
fn list_in(
    environment: DatapackEnvironment,
    records: &[Record],
    prepare: impl FnOnce(&DatapackEnvironment),
) -> Output {
    environment.write_mism_toml(&mism_toml_with_records("paper", "26.3", &[], records));
    prepare(&environment);
    environment.mism_datapack(["list"]).run()
}

/// `mism datapack list` on a Paper 26.3 server with the datapack records `records`, after
/// `prepare`.
fn list_with(records: &[Record], prepare: impl FnOnce(&DatapackEnvironment)) -> Output {
    list_in(
        DatapackEnvironment::with_server("paper", "26.3"),
        records,
        prepare,
    )
}

/// dlist-1: each record and each datapack file has its own line.
#[test]
fn each_record_and_file_has_a_line() {
    let sha256 = sha256_hex(&true_ending_datapack());

    let output = list_with(&[true_ending_record(&sha256)], |environment| {
        environment.put_in_datapacks(TRUE_ENDING_FILE_NAME, &true_ending_datapack());
        environment.put_in_datapacks("Extra.zip", b"extra");
    });

    assert_eq!(
        output.stdout.lines().count(),
        2,
        "stdout: {}",
        output.stdout
    );
}

/// dlist-1: listing exits with status 0.
#[test]
fn listing_exits_with_status_0() {
    let output = list_with(&[], |_| {});

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dlist-1: the plugin records are not datapack records.
#[test]
fn plugin_records_are_not_listed() {
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    environment.write_mism_toml(&mism_toml_with_records(
        "paper",
        "26.3",
        &[Record {
            name: "LuckPerms",
            source: "modrinth:luckperms:v5.5.71-bukkit",
            file_name: "LuckPerms-Bukkit-5.5.71.jar",
            sha256: &sha256_hex(b"LuckPerms"),
        }],
        &[],
    ));

    let output = environment.mism_datapack(["list"]).run();

    assert_eq!((output.exit_code, output.stdout.as_str()), (0, ""));
}

/// dlist-1, dp-3: the records are matched against the world directory of the current
/// `level-name`.
#[test]
fn records_are_matched_against_the_current_world_directory() {
    let sha256 = sha256_hex(&true_ending_datapack());
    let environment = DatapackEnvironment::with_server("paper", "26.3");
    environment.put_in_datapacks(TRUE_ENDING_FILE_NAME, &true_ending_datapack());

    let output = list_in(
        environment.with_level_name("other"),
        &[true_ending_record(&sha256)],
        |_| {},
    );

    assert_eq!(
        output.stdout,
        format!(
            "mism: missing\t{TRUE_ENDING_NAME}\t1.1.4d\t{TRUE_ENDING_FILE_NAME}\t{TRUE_ENDING_SOURCE}\n"
        )
    );
}

/// dlist-1 (「サーバーのデータパックの形式の番号」 方法): listing does not read the server jar.
#[test]
fn listing_needs_no_server_jar() {
    let output = list_in(
        DatapackEnvironment::with_server_without_jar("paper", "26.3"),
        &[],
        |_| {},
    );

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dlist-2: the example of the specification: a recorded file with the recorded SHA-256 is
/// `ok`.
#[test]
fn file_with_the_recorded_sha256_is_ok() {
    let sha256 = sha256_hex(&true_ending_datapack());

    let output = list_with(&[true_ending_record(&sha256)], |environment| {
        environment.put_in_datapacks(TRUE_ENDING_FILE_NAME, &true_ending_datapack());
    });

    assert_eq!(
        output.stdout,
        format!(
            "mism: ok\t{TRUE_ENDING_NAME}\t1.1.4d\t{TRUE_ENDING_FILE_NAME}\t{TRUE_ENDING_SOURCE}\n"
        )
    );
}

/// dlist-2: a record whose file is not in `datapacks/` is `missing`.
#[test]
fn record_without_its_file_is_missing() {
    let sha256 = sha256_hex(&true_ending_datapack());

    let output = list_with(&[true_ending_record(&sha256)], |_| {});

    assert_eq!(
        output.stdout,
        format!(
            "mism: missing\t{TRUE_ENDING_NAME}\t1.1.4d\t{TRUE_ENDING_FILE_NAME}\t{TRUE_ENDING_SOURCE}\n"
        )
    );
}

/// dlist-2: a recorded file with another SHA-256 is `changed`.
#[test]
fn file_with_another_sha256_is_changed() {
    let sha256 = sha256_hex(&true_ending_datapack());

    let output = list_with(&[true_ending_record(&sha256)], |environment| {
        environment.put_in_datapacks(TRUE_ENDING_FILE_NAME, b"changed");
    });

    assert_eq!(
        output.stdout,
        format!(
            "mism: changed\t{TRUE_ENDING_NAME}\t1.1.4d\t{TRUE_ENDING_FILE_NAME}\t{TRUE_ENDING_SOURCE}\n"
        )
    );
}

/// dlist-2: a file without a record is `unmanaged`, named by its file name without `.zip`, and
/// has no version number and no source.
#[test]
fn file_without_a_record_is_unmanaged() {
    let output = list_with(&[], |environment| {
        environment.put_in_datapacks("Extra.zip", b"extra");
    });

    assert_eq!(output.stdout, "mism: unmanaged\tExtra\t-\tExtra.zip\t-\n");
}

/// dlist-2: a file whose name does not end with `.zip` is not listed.
#[test]
fn file_not_named_zip_is_not_listed() {
    let output = list_with(&[], |environment| {
        environment.put_in_datapacks("notes.txt", b"notes");
        environment.put_in_datapacks("Extra.jar", b"extra");
    });

    assert_eq!((output.exit_code, output.stdout.as_str()), (0, ""));
}

/// dlist-2 (「プラグインとデータパックを置くディレクトリ」 方法): a symbolic link to a file is a
/// file in `datapacks/`.
#[test]
fn symbolic_link_to_a_file_is_listed() {
    let output = list_with(&[], |environment| {
        let target = environment.write_local_zip("Target.zip", b"target");
        fs::create_dir_all(environment.datapacks()).unwrap();
        symlink(target, environment.datapacks().join("Linked.zip")).unwrap();
    });

    assert_eq!(output.stdout, "mism: unmanaged\tLinked\t-\tLinked.zip\t-\n");
}

/// d-dpdir-4: a directory in `datapacks/`, even one whose name ends with `.zip`, is not listed.
#[test]
fn directories_in_datapacks_are_not_listed() {
    let output = list_with(&[], |environment| {
        fs::create_dir_all(environment.datapacks().join("Unpacked/data")).unwrap();
        fs::write(
            environment.datapacks().join("Unpacked/pack.mcmeta"),
            r#"{"pack": {"description": "Unpacked", "pack_format": 48}}"#,
        )
        .unwrap();
        fs::create_dir_all(environment.datapacks().join("Folder.zip")).unwrap();
    });

    assert_eq!((output.exit_code, output.stdout.as_str()), (0, ""));
}

/// dlist-2 (「プラグインとデータパックを置くディレクトリ」 方法): the records in their order
/// precede the unmanaged files in the order of their names.
#[test]
fn records_in_their_order_precede_unmanaged_files_by_name() {
    let sha256 = sha256_hex(b"recorded");
    let records = [
        Record {
            name: "B",
            source: "https://example.com/B.zip",
            file_name: "B.zip",
            sha256: &sha256,
        },
        Record {
            name: "A",
            source: "../A.zip",
            file_name: "A.zip",
            sha256: &sha256,
        },
    ];

    let output = list_with(&records, |environment| {
        environment.put_in_datapacks("D.zip", b"d");
        environment.put_in_datapacks("C.zip", b"c");
    });

    assert_eq!(
        output.stdout,
        "mism: missing\tB\t-\tB.zip\thttps://example.com/B.zip\n\
         mism: missing\tA\t-\tA.zip\t../A.zip\n\
         mism: unmanaged\tC\t-\tC.zip\t-\n\
         mism: unmanaged\tD\t-\tD.zip\t-\n"
    );
}

/// dlist-3: `--global` prints each datapack in the cache directory.
#[test]
fn global_prints_each_cached_datapack() {
    let environment = DatapackEnvironment::without_server();
    let zip_a = environment.write_local_zip("A.zip", b"a");
    let zip_b = environment.write_local_zip("B.zip", b"b");
    environment.fetch(zip_a.to_str().unwrap());
    environment.fetch(zip_b.to_str().unwrap());

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!(
        output.stdout.lines().count(),
        2,
        "stdout: {}",
        output.stdout
    );
}

/// dlist-3: `--global` prints the datapacks only, not the plugins in the cache directory.
#[test]
fn global_prints_no_plugin() {
    let environment = DatapackEnvironment::without_server();
    let jar = environment.write_outside("Plugin.jar", b"plugin");
    environment
        .mism(
            "plugin",
            &environment.server(),
            ["fetch", jar.to_str().unwrap()],
        )
        .run();

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!((output.exit_code, output.stdout.as_str()), (0, ""));
}

/// dlist-3: `--global` needs no server directory and exits with status 0.
#[test]
fn global_exits_with_status_0_without_a_server() {
    let environment = DatapackEnvironment::without_server();

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!(output.exit_code, 0, "stdout: {}", output.stdout);
}

/// dlist-4: a datapack from Modrinth has its name, version number, SHA-256, and source.
#[test]
fn global_record_of_a_modrinth_datapack() {
    let environment = DatapackEnvironment::without_server();
    environment
        .modrinth
        .serve_true_ending(&true_ending_datapack());
    environment.fetch(TRUE_ENDING_SOURCE);

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: {TRUE_ENDING_NAME}\t1.1.4d\t{}\t{TRUE_ENDING_SOURCE}\n",
            sha256_hex(&true_ending_datapack())
        )
    );
}

/// dlist-4: a datapack not from Modrinth has `-` as its version number.
#[test]
fn global_record_of_a_local_datapack() {
    let environment = DatapackEnvironment::without_server();
    environment.write_local_zip("A.zip", b"a");
    environment.fetch("../outside/A.zip");

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!("mism: A\t-\t{}\t../outside/A.zip\n", sha256_hex(b"a"))
    );
}

/// d-list-1
#[test]
fn global_list_is_ordered_by_name_then_sha256() {
    let environment = DatapackEnvironment::without_server();
    let first_a = environment.modrinth.mount_jar("/one/A.zip", b"first");
    let second_a = environment.modrinth.mount_jar("/two/A.zip", b"second");
    let zip_b = environment.write_local_zip("B.zip", b"b");
    environment.fetch(zip_b.to_str().unwrap());
    environment.fetch(&first_a);
    environment.fetch(&second_a);
    let mut a_records = [
        (sha256_hex(b"first"), first_a),
        (sha256_hex(b"second"), second_a),
    ];
    a_records.sort();

    let output = environment.mism_datapack(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: A\t-\t{}\t{}\nmism: A\t-\t{}\t{}\nmism: B\t-\t{}\t{}\n",
            a_records[0].0,
            a_records[0].1,
            a_records[1].0,
            a_records[1].1,
            sha256_hex(b"b"),
            zip_b.display()
        )
    );
}
