use bms_package::{Manifest, PackageBuilder};
use bms_package_manager::{
    BmsServeServer, DownloadProgressCallback, HttpClient, PackageManager,
    RegistrySource, RemotePackageInstaller, RemotePackageMetadata,
    RemoteRegistryIndex, SourcesConfig,
};
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

fn get_bpm_exe() -> &'static str {
    env!("CARGO_BIN_EXE_bpm")
}

fn create_temp_dir(prefix: &str) -> PathBuf {
    let temp = std::env::temp_dir().join(format!(
        "{}_{}_{}",
        prefix,
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp).unwrap();
    temp
}

fn create_dummy_package(id: &str, name: &str) -> Vec<u8> {
    let manifest = Manifest::new(id, name).with_author("Test Author");
    let mut builder = PackageBuilder::new(manifest);
    builder
        .add_file("bms/test.bms", b"#TITLE Test Song\n#BPM 160\n#WAV01 01.wav".to_vec())
        .unwrap();
    builder
        .add_file("audio/01.wav", vec![0x52, 0x49, 0x46, 0x46, 0x01, 0x02, 0x03])
        .unwrap();
    builder.build_to_bytes().unwrap()
}

fn make_remote_pkg(
    id: &str,
    title: &str,
    sha256: &str,
    state_hash: &str,
    size_bytes: u64,
    download_url: &str,
) -> RemotePackageMetadata {
    RemotePackageMetadata {
        id: id.to_string(),
        version: "1.0.0".to_string(),
        state_hash: state_hash.to_string(),
        title: title.to_string(),
        artist: "Test Artist".to_string(),
        genre: Some("Electronic".to_string()),
        bpm: Some(150.0),
        play_levels: vec![7],
        keysounds_count: None,
        size_bytes,
        sha256: sha256.to_string(),
        download_url: download_url.to_string(),
        preview_audio_url: None,
        banner_image_url: None,
        companion_bga: None,
    }
}

fn make_remote_index(name: &str, packages: Vec<RemotePackageMetadata>) -> RemoteRegistryIndex {
    RemoteRegistryIndex {
        format_version: "1.0.0".to_string(),
        name: name.to_string(),
        description: None,
        url: "http://example.com/index.json".to_string(),
        updated_at: "2026-09-14T00:00:00Z".to_string(),
        packages,
    }
}

#[derive(Default)]
struct TestCallback {
    total_downloaded: u64,
}

impl DownloadProgressCallback for TestCallback {
    fn on_progress(&mut self, downloaded_bytes: u64, _total_bytes: Option<u64>) {
        self.total_downloaded = downloaded_bytes;
    }
}

#[test]
fn test_e2e_remote_index_multi_source_priority_merge() {
    let src1 = RegistrySource {
        id: "source-low".to_string(),
        name: "Low Priority Source".to_string(),
        url: "http://example.com/low/index.json".to_string(),
        enabled: true,
        priority: 50,
    };
    let src2 = RegistrySource {
        id: "source-high".to_string(),
        name: "High Priority Source".to_string(),
        url: "http://example.com/high/index.json".to_string(),
        enabled: true,
        priority: 150,
    };

    let index_low = make_remote_index(
        "Low Index",
        vec![
            make_remote_pkg(
                "shared-song",
                "Shared Song (Low Priority Version)",
                &"0".repeat(64),
                "state-low",
                1000,
                "packages/shared-low.bmsp",
            ),
            make_remote_pkg(
                "unique-low",
                "Unique Low Song",
                &"1".repeat(64),
                "state-unique-low",
                2000,
                "packages/unique-low.bmsp",
            ),
        ],
    );

    let index_high = make_remote_index(
        "High Index",
        vec![make_remote_pkg(
            "shared-song",
            "Shared Song (High Priority Winner)",
            &"2".repeat(64),
            "state-high",
            3000,
            "packages/shared-high.bmsp",
        )],
    );

    let pairs = [(&src1, &index_low), (&src2, &index_high)];
    let merged = SourcesConfig::merge_packages(&pairs);

    assert_eq!(merged.len(), 2);
    let shared = merged.iter().find(|p| p.id == "shared-song").unwrap();
    assert_eq!(shared.title, "Shared Song (High Priority Winner)");
    assert_eq!(shared.state_hash, "state-high");

    let unique = merged.iter().find(|p| p.id == "unique-low").unwrap();
    assert_eq!(unique.title, "Unique Low Song");
}

#[test]
fn test_e2e_streaming_download_and_checksum_verification() {
    let pkg_bytes = create_dummy_package("test-stream", "Stream Test Song");
    let sha256_hash = bms_package::sha256_hex(&pkg_bytes);
    let pkg_len = pkg_bytes.len() as u64;

    // Spin up mini TCP server
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().unwrap().port();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();
    let bytes_to_serve = pkg_bytes.clone();

    let server_handle = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !stop_flag_clone.load(Ordering::Relaxed) {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::Read;
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    bytes_to_serve.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&bytes_to_serve);
                let _ = stream.flush();
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    let storage = create_temp_dir("bpm_e2e_download");
    let installer = RemotePackageInstaller::new(&storage);
    let client = HttpClient::default();

    let metadata = make_remote_pkg(
        "test-stream",
        "Stream Test Song",
        &sha256_hash,
        "dummy_state",
        pkg_len,
        &format!("http://127.0.0.1:{}/package.bmsp", port),
    );

    let callback = TestCallback::default();
    let temp_file = installer
        .download_package(&client, &metadata, "", callback)
        .expect("download should succeed with matching checksum");

    assert!(temp_file.path().exists());
    let downloaded_len = fs::metadata(temp_file.path()).unwrap().len();
    assert_eq!(downloaded_len, pkg_len);

    let committed_path = temp_file.commit();
    assert!(committed_path.exists());

    // Clean up
    let _ = fs::remove_file(committed_path);
    stop_flag.store(true, Ordering::Relaxed);
    let _ = server_handle.join();
    let _ = fs::remove_dir_all(&storage);
}

#[test]
fn test_e2e_download_corrupted_checksum_and_dropguard_cleanup() {
    let pkg_bytes = create_dummy_package("test-corrupt", "Corrupt Song");
    let wrong_sha256 = "f".repeat(64); // Mismatched SHA-256 hash

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().unwrap().port();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();
    let bytes_to_serve = pkg_bytes.clone();

    let server_handle = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !stop_flag_clone.load(Ordering::Relaxed) {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::Read;
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    bytes_to_serve.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&bytes_to_serve);
                let _ = stream.flush();
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    let storage = create_temp_dir("bpm_e2e_corrupt");
    let installer = RemotePackageInstaller::new(&storage);
    let client = HttpClient::default();

    let metadata = make_remote_pkg(
        "test-corrupt",
        "Corrupt Song",
        &wrong_sha256,
        "dummy_state",
        pkg_bytes.len() as u64,
        &format!("http://127.0.0.1:{}/package.bmsp", port),
    );

    let result = installer.download_package(
        &client,
        &metadata,
        "",
        bms_package_manager::NoopProgressCallback,
    );
    assert!(result.is_err(), "Must fail with checksum mismatch");

    // Verify DropGuard deleted any .tmp files in .cache/downloads
    let downloads_dir = installer.downloads_dir();
    if downloads_dir.exists() {
        let entries = fs::read_dir(downloads_dir).unwrap();
        let tmp_count = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "tmp"))
            .count();
        assert_eq!(tmp_count, 0, "All .tmp files must be swept by DropGuard");
    }

    stop_flag.store(true, Ordering::Relaxed);
    let _ = server_handle.join();
    let _ = fs::remove_dir_all(&storage);
}

#[test]
fn test_e2e_bpm_serve_and_bpm_install_loopback() {
    // 1. Prepare server storage with an installed package
    let server_storage = create_temp_dir("bpm_e2e_server");
    let mut server_pm = PackageManager::new(&server_storage).unwrap();
    let pkg_bytes = create_dummy_package("loopback-song", "Loopback BMS Song");
    let tmp_bmsp = server_storage.join("loopback-song.bmsp");
    fs::write(&tmp_bmsp, &pkg_bytes).unwrap();
    let installed_rec = server_pm.install(&tmp_bmsp).unwrap();
    assert_eq!(installed_rec.id, "loopback-song");

    // 2. Start BmsServeServer on ephemeral port
    let mut server = BmsServeServer::start(server_storage.clone(), "127.0.0.1", 0)
        .expect("start BmsServeServer");
    let actual_port = server.port();
    let base_url = format!("http://127.0.0.1:{actual_port}");

    // Give server a short moment to start listening
    thread::sleep(Duration::from_millis(50));

    // 3. Prepare client storage and configure source pointing to server
    let client_storage = create_temp_dir("bpm_e2e_client");
    let sources_path = client_storage.join("sources.json");
    let mut client_sources = SourcesConfig::default();
    client_sources.sources.clear(); // remove official default
    client_sources.add_or_update(
        "local-serve",
        "Local Serve Hub",
        format!("{base_url}/index.json"),
        100,
    );
    client_sources.save_to_file(&sources_path).unwrap();

    // 4. Client runs `bpm update`
    let update_output = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &client_storage)
        .arg("update")
        .output()
        .expect("run bpm update");
    let update_str = String::from_utf8_lossy(&update_output.stdout);
    assert!(
        update_str.contains("OK (1 packages indexed)")
            || update_str.contains("Registry update complete"),
        "Update output: {update_str}"
    );

    // 5. Client runs `bpm install loopback-song`
    let install_output = Command::new(get_bpm_exe())
        .env("BEETLE_PACKAGES_DIR", &client_storage)
        .args(["install", "loopback-song"])
        .output()
        .expect("run bpm install loopback-song");
    let install_str = String::from_utf8_lossy(&install_output.stdout);
    assert!(
        install_str.contains("Installed 'Loopback BMS Song'")
            || install_str.contains("loopback-song"),
        "Install output: {install_str}"
    );

    // 6. Verify client PackageManager has package installed
    let client_pm = PackageManager::new(&client_storage).unwrap();
    let client_pkg = client_pm.registry().get_package("loopback-song");
    assert!(
        client_pkg.is_some(),
        "loopback-song must be installed on client"
    );
    let pkg_rec = client_pkg.unwrap();
    assert_eq!(pkg_rec.name, "Loopback BMS Song");
    assert_eq!(pkg_rec.active_state, installed_rec.state_hash);

    // 7. Stop server
    server.stop();

    // Clean up temp dirs
    let _ = fs::remove_dir_all(&server_storage);
    let _ = fs::remove_dir_all(&client_storage);
}
