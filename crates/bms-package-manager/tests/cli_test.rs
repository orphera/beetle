use std::fs;
use std::process::Command;
use std::time::SystemTime;

fn get_bpm_exe() -> &'static str {
    env!("CARGO_BIN_EXE_bpm")
}

fn create_temp_storage() -> std::path::PathBuf {
    let temp_dir = std::env::temp_dir().join(format!(
        "bpm_cli_test_{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_dir).unwrap();
    temp_dir
}

#[test]
fn test_cli_usage_contains_remote_commands() {
    let output = Command::new(get_bpm_exe())
        .output()
        .expect("failed to execute bpm");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bpm update"));
    assert!(stdout.contains("bpm search <query>"));
    assert!(stdout.contains("bpm upgrade"));
    assert!(stdout.contains("bpm source <list|add|remove>"));
    assert!(stdout.contains("bpm serve"));
}

#[test]
fn test_cli_source_management_lifecycle() {
    let storage = create_temp_storage();

    // 1. List default sources
    let list_out = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &storage)
        .arg("source")
        .arg("list")
        .output()
        .expect("failed to list sources");
    let list_stdout = String::from_utf8_lossy(&list_out.stdout);
    assert!(list_stdout.contains("official"));

    // 2. Add custom source
    let add_out = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &storage)
        .args([
            "source",
            "add",
            "lan-hub",
            "http://127.0.0.1:8080/index.json",
            "--name",
            "LAN Hub",
            "--priority",
            "200",
        ])
        .output()
        .expect("failed to add source");
    let add_stdout = String::from_utf8_lossy(&add_out.stdout);
    assert!(add_stdout.contains("Successfully added/updated registry source 'lan-hub'"));

    // 3. Verify in source list
    let list_out2 = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &storage)
        .args(["source", "list"])
        .output()
        .expect("failed to list sources");
    let list_stdout2 = String::from_utf8_lossy(&list_out2.stdout);
    assert!(list_stdout2.contains("lan-hub"));
    assert!(list_stdout2.contains("200"));

    // 4. Remove source
    let rm_out = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &storage)
        .args(["source", "remove", "lan-hub"])
        .output()
        .expect("failed to remove source");
    let rm_stdout = String::from_utf8_lossy(&rm_out.stdout);
    assert!(rm_stdout.contains("Successfully removed registry source 'lan-hub'"));

    // 5. Verify removed
    let list_out3 = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &storage)
        .args(["source", "list"])
        .output()
        .expect("failed to list sources");
    let list_stdout3 = String::from_utf8_lossy(&list_out3.stdout);
    assert!(!list_stdout3.contains("lan-hub"));

    let _ = fs::remove_dir_all(&storage);
}

#[test]
fn test_cli_search_and_upgrade_with_cached_index() {
    let storage = create_temp_storage();

    // Set up a mock cached index
    let cache_dir = storage.join(".cache").join("registry");
    fs::create_dir_all(&cache_dir).unwrap();

    let sample_index_json = r#"{
        "format_version": "1.0.0",
        "name": "Beetle Official Registry",
        "url": "https://packages.beetle-engine.org/index.json",
        "updated_at": "2026-09-14T00:00:00Z",
        "packages": [
            {
                "id": "org.beetle.conflict",
                "version": "1.0.0",
                "state_hash": "a1b2c3d4e5f607182930405060708090a1b2c3d4e5f607182930405060708090",
                "title": "Conflict",
                "artist": "SiHanatsuka",
                "genre": "Hardcore",
                "bpm": 160.0,
                "play_levels": [5, 10, 12],
                "keysounds_count": 850,
                "size_bytes": 10485760,
                "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "download_url": "https://packages.beetle-engine.org/packages/conflict.bmsp"
            }
        ]
    }"#;
    fs::write(cache_dir.join("official.json"), sample_index_json).unwrap();

    // 1. Search for conflict
    let search_out = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &storage)
        .args(["search", "conflict"])
        .output()
        .expect("failed to search");
    let search_stdout = String::from_utf8_lossy(&search_out.stdout);
    assert!(search_stdout.contains("org.beetle.conflict"));
    assert!(search_stdout.contains("Conflict"));
    assert!(search_stdout.contains("SiHanatsuka"));
    assert!(search_stdout.contains("[Available]"));

    // 2. Search for nonexistent
    let search_empty = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &storage)
        .args(["search", "nonexistent"])
        .output()
        .expect("failed to search");
    let empty_stdout = String::from_utf8_lossy(&search_empty.stdout);
    assert!(empty_stdout.contains("No remote packages matching 'nonexistent' found."));

    // 3. Run upgrade when nothing is installed
    let upgrade_out = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &storage)
        .arg("upgrade")
        .output()
        .expect("failed to run upgrade");
    let upgrade_stdout = String::from_utf8_lossy(&upgrade_out.stdout);
    assert!(upgrade_stdout.contains("All installed packages are up to date."));

    let _ = fs::remove_dir_all(&storage);
}

#[test]
fn test_cli_library_add_list_remove() {
    let dir = create_temp_storage();
    let file = dir.join("library.txt");
    let bms = dir.join("old_bms");
    fs::create_dir_all(&bms).unwrap();
    let run = |args: &[&str]| {
        Command::new(get_bpm_exe())
            .env("BEETLE_LIBRARY_FILE", &file)
            .arg("library")
            .args(args)
            .output()
            .expect("failed to run bpm library")
    };
    let bms_str = bms.to_str().unwrap();

    assert!(run(&["add", bms_str]).status.success());
    // Same folder again is a no-op, not an error or a duplicate line.
    assert!(run(&["add", bms_str]).status.success());
    let listed = String::from_utf8_lossy(&run(&["list"]).stdout).into_owned();
    assert!(listed.contains("[ok]") && listed.contains("old_bms"), "{listed}");
    assert_eq!(fs::read_to_string(&file).unwrap().matches("old_bms").count(), 1);

    assert!(!run(&["add", dir.join("nope").to_str().unwrap()]).status.success());

    assert!(run(&["remove", bms_str]).status.success());
    assert!(!run(&["remove", bms_str]).status.success());
    assert!(!fs::read_to_string(&file).unwrap().contains("old_bms"));
}
