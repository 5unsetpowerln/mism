mod common;

use common::fake_modrinth::{LUCKPERMS_FILE_NAME, LUCKPERMS_SOURCE, sha256_hex};
use common::mism_process::Output;
use common::plugin_server_directory::{PluginEnvironment, Record, mism_toml_text};
use common::test_plugin_jar::{jar_with_plugin_yml, plugin_yml};

fn luckperms_jar() -> Vec<u8> {
    jar_with_plugin_yml(&plugin_yml("LuckPerms", "5.5.71", "1.13"))
}

/// The record of LuckPerms v5.5.71-bukkit with the SHA-256 `sha256`.
fn luckperms_record(sha256: &str) -> Record<'_> {
    Record {
        name: "LuckPerms",
        source: LUCKPERMS_SOURCE,
        file_name: LUCKPERMS_FILE_NAME,
        sha256,
    }
}

/// `mism plugin list` on a Paper 26.3 server with `records`, after `prepare`.
fn list_with(records: &[Record], prepare: impl FnOnce(&PluginEnvironment)) -> Output {
    let environment = PluginEnvironment::with_server("paper", "26.3");
    environment.write_mism_toml(&mism_toml_text("paper", "26.3", records));
    prepare(&environment);
    environment.mism_plugin(["list"]).run()
}

/// list-1: each record and each jar has its own line.
#[test]
fn each_record_and_jar_has_a_line() {
    let sha256 = sha256_hex(&luckperms_jar());

    let output = list_with(&[luckperms_record(&sha256)], |environment| {
        environment.put_in_plugins(LUCKPERMS_FILE_NAME, &luckperms_jar());
        environment.put_in_plugins("Extra.jar", b"extra");
    });

    assert_eq!(output.stdout.lines().count(), 2);
}

/// list-1: listing exits with status 0.
#[test]
fn listing_exits_with_status_0() {
    let output = list_with(&[], |_| {});

    assert_eq!(output.exit_code, 0);
}

/// list-2: a recorded jar with the recorded SHA-256 is `ok`.
#[test]
fn jar_with_the_recorded_sha256_is_ok() {
    let sha256 = sha256_hex(&luckperms_jar());

    let output = list_with(&[luckperms_record(&sha256)], |environment| {
        environment.put_in_plugins(LUCKPERMS_FILE_NAME, &luckperms_jar());
    });

    assert_eq!(
        output.stdout,
        format!("mism: ok\tLuckPerms\tv5.5.71-bukkit\t{LUCKPERMS_FILE_NAME}\t{LUCKPERMS_SOURCE}\n")
    );
}

/// list-2: a record whose jar is not in `plugins/` is `missing`.
#[test]
fn record_without_its_jar_is_missing() {
    let sha256 = sha256_hex(&luckperms_jar());

    let output = list_with(&[luckperms_record(&sha256)], |_| {});

    assert_eq!(
        output.stdout,
        format!(
            "mism: missing\tLuckPerms\tv5.5.71-bukkit\t{LUCKPERMS_FILE_NAME}\t{LUCKPERMS_SOURCE}\n"
        )
    );
}

/// list-2: a recorded jar with another SHA-256 is `changed`.
#[test]
fn jar_with_another_sha256_is_changed() {
    let sha256 = sha256_hex(&luckperms_jar());

    let output = list_with(&[luckperms_record(&sha256)], |environment| {
        environment.put_in_plugins(LUCKPERMS_FILE_NAME, b"changed");
    });

    assert_eq!(
        output.stdout,
        format!(
            "mism: changed\tLuckPerms\tv5.5.71-bukkit\t{LUCKPERMS_FILE_NAME}\t{LUCKPERMS_SOURCE}\n"
        )
    );
}

/// list-2: a jar without a record is `unmanaged`, named by its file name without `.jar`, and
/// has no version number and no source.
#[test]
fn jar_without_a_record_is_unmanaged() {
    let output = list_with(&[], |environment| {
        environment.put_in_plugins("Extra.jar", b"extra");
    });

    assert_eq!(output.stdout, "mism: unmanaged\tExtra\t-\tExtra.jar\t-\n");
}

/// d-pdir-2
#[test]
fn records_in_their_order_precede_unmanaged_jars_by_name() {
    let sha256 = sha256_hex(b"recorded");
    let records = [
        Record {
            name: "B",
            source: "https://example.com/B.jar",
            file_name: "B.jar",
            sha256: &sha256,
        },
        Record {
            name: "A",
            source: "../A.jar",
            file_name: "A.jar",
            sha256: &sha256,
        },
    ];

    let output = list_with(&records, |environment| {
        environment.put_in_plugins("D.jar", b"d");
        environment.put_in_plugins("C.jar", b"c");
    });

    assert_eq!(
        output.stdout,
        "mism: missing\tB\t-\tB.jar\thttps://example.com/B.jar\n\
         mism: missing\tA\t-\tA.jar\t../A.jar\n\
         mism: unmanaged\tC\t-\tC.jar\t-\n\
         mism: unmanaged\tD\t-\tD.jar\t-\n"
    );
}

/// list-3: `--global` prints each plugin in the cache directory.
#[test]
fn global_prints_each_cached_plugin() {
    let environment = PluginEnvironment::without_server();
    let jar_a = environment.write_local_jar("A.jar", b"a");
    let jar_b = environment.write_local_jar("B.jar", b"b");
    environment.fetch(jar_a.to_str().unwrap());
    environment.fetch(jar_b.to_str().unwrap());

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(output.stdout.lines().count(), 2);
}

/// list-3: `--global` needs no server directory and exits with status 0.
#[test]
fn global_exits_with_status_0_without_a_server() {
    let environment = PluginEnvironment::without_server();

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(output.exit_code, 0);
}

/// list-4: a plugin from Modrinth has its name, version number, SHA-256, and source.
#[test]
fn global_record_of_a_modrinth_plugin() {
    let environment = PluginEnvironment::without_server();
    environment.modrinth.serve_luckperms(&luckperms_jar());
    environment.fetch(LUCKPERMS_SOURCE);

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: LuckPerms\tv5.5.71-bukkit\t{}\t{LUCKPERMS_SOURCE}\n",
            sha256_hex(&luckperms_jar())
        )
    );
}

/// list-4: a plugin not from Modrinth has `-` as its version number.
#[test]
fn global_record_of_a_local_plugin() {
    let environment = PluginEnvironment::without_server();
    environment.write_local_jar("A.jar", b"a");
    environment.fetch("../outside/A.jar");

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!("mism: A\t-\t{}\t../outside/A.jar\n", sha256_hex(b"a"))
    );
}

/// d-list-1
#[test]
fn global_list_is_ordered_by_name_then_sha256() {
    let environment = PluginEnvironment::without_server();
    let first_a = environment.modrinth.mount_jar("/one/A.jar", b"first");
    let second_a = environment.modrinth.mount_jar("/two/A.jar", b"second");
    let jar_b = environment.write_local_jar("B.jar", b"b");
    environment.fetch(jar_b.to_str().unwrap());
    environment.fetch(&first_a);
    environment.fetch(&second_a);
    let mut a_records = [
        (sha256_hex(b"first"), first_a),
        (sha256_hex(b"second"), second_a),
    ];
    a_records.sort();

    let output = environment.mism_plugin(["list", "--global"]).run();

    assert_eq!(
        output.stdout,
        format!(
            "mism: A\t-\t{}\t{}\nmism: A\t-\t{}\t{}\nmism: B\t-\t{}\t{}\n",
            a_records[0].0,
            a_records[0].1,
            a_records[1].0,
            a_records[1].1,
            sha256_hex(b"b"),
            jar_b.display()
        )
    );
}
