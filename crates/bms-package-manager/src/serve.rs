use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use bms_package::Sha256Hasher;

use crate::manager::PackageManager;
use crate::registry::remote::{CompanionBgaMetadata, RemotePackageMetadata, RemoteRegistryIndex};

/// Detects local LAN IPv4 address without external dependencies.
/// Uses a dummy UDP connect to a public IP, querying the OS routing table.
pub fn get_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// Computes SHA-256 checksum for a file on disk using streaming Sha256Hasher.
pub fn compute_file_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize_hex())
}

/// Internal serve context containing the synthesized index and package file route map.
#[derive(Debug, Clone)]
pub struct ServeState {
    pub index_json: String,
    pub files: HashMap<String, PathBuf>,
}

/// Builds the serve state by synthesizing an index from the local package storage.
pub fn build_serve_state(storage_dir: &Path, base_url: &str) -> Result<ServeState, String> {
    let mut files = HashMap::new();
    let mut packages = Vec::new();

    if let Ok(manager) = PackageManager::new(storage_dir) {
        for record in manager.list_active_packages() {
            let state_dir = &record.location;
            let bmsp_path = state_dir.join("package.bmsp");
            if bmsp_path.exists() {
                let size_bytes = fs::metadata(&bmsp_path)
                    .map(|m| m.len())
                    .unwrap_or_default();
                let sha256 = compute_file_sha256(&bmsp_path).map_err(|e| {
                    format!("Failed to compute hash for '{}': {e}", bmsp_path.display())
                })?;

                let filename = format!("{}.bmsp", record.id);
                let download_url = format!("{base_url}/packages/{filename}");

                // Check for companion BGA package
                let mut companion_bga = None;
                if record.bga_status == crate::registry::BgaStatus::Companion {
                    let bga_filename = format!("{}.bga.bmsp", record.id);
                    let bga_file = state_dir.join(&bga_filename);
                    if bga_file.exists() {
                        let bga_size = fs::metadata(&bga_file).map(|m| m.len()).unwrap_or_default();
                        if let Ok(bga_sha) = compute_file_sha256(&bga_file) {
                            let bga_download_url = format!("{base_url}/packages/{bga_filename}");
                            files.insert(bga_filename, bga_file);
                            companion_bga = Some(CompanionBgaMetadata {
                                id: format!("{}.bga", record.id),
                                size_bytes: bga_size,
                                sha256: bga_sha,
                                download_url: bga_download_url,
                            });
                        }
                    }
                }

                files.insert(filename, bmsp_path);

                packages.push(RemotePackageMetadata {
                    id: record.id.clone(),
                    version: "1.0.0".to_string(),
                    state_hash: record.state_hash.clone(),
                    title: record.name.clone(),
                    artist: record.author.unwrap_or_else(|| "Unknown".to_string()),
                    genre: None,
                    bpm: None,
                    play_levels: vec![],
                    keysounds_count: None,
                    size_bytes,
                    sha256,
                    download_url,
                    preview_audio_url: None,
                    banner_image_url: None,
                    companion_bga,
                });
            }
        }
    }

    // Also check storage_dir directly for any standalone .bmsp files
    if let Ok(entries) = fs::read_dir(storage_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "bmsp") {
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !fname.ends_with(".bga.bmsp") && !files.contains_key(fname) {
                    if let Ok(pkg) = bms_package::Package::open(&path) {
                        let manifest = pkg.manifest().clone();
                        let state_hash = pkg.state_hash();
                        let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or_default();
                        if let Ok(sha256) = compute_file_sha256(&path) {
                            let download_url = format!("{base_url}/packages/{fname}");
                            files.insert(fname.to_string(), path);

                            packages.push(RemotePackageMetadata {
                                id: manifest.id,
                                version: "1.0.0".to_string(),
                                state_hash,
                                title: manifest.name,
                                artist: manifest.author.unwrap_or_else(|| "Unknown".to_string()),
                                genre: None,
                                bpm: None,
                                play_levels: vec![],
                                keysounds_count: None,
                                size_bytes,
                                sha256,
                                download_url,
                                preview_audio_url: None,
                                banner_image_url: None,
                                companion_bga: None,
                            });
                        }
                    }
                }
            }
        }
    }

    let index = RemoteRegistryIndex {
        format_version: "1.0.0".to_string(),
        name: "Beetle Local LAN Hub".to_string(),
        description: Some("Hosted by bpm serve".to_string()),
        url: format!("{base_url}/index.json"),
        updated_at: "2026-09-14T00:00:00Z".to_string(),
        packages,
    };

    let index_json = index
        .to_json_pretty()
        .map_err(|e| format!("Failed to serialize registry index: {e}"))?;

    Ok(ServeState { index_json, files })
}

/// Lightweight static HTTP/1.1 server hosting a local package registry for LAN sharing.
pub struct BmsServeServer {
    port: u16,
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl BmsServeServer {
    /// Starts the serve server on the specified bind address and port.
    pub fn start(storage_dir: PathBuf, bind_addr: &str, port: u16) -> Result<Self, String> {
        let listener = TcpListener::bind(format!("{bind_addr}:{port}"))
            .map_err(|e| format!("Failed to bind to {bind_addr}:{port}: {e}"))?;
        let actual_port = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local port: {e}"))?
            .port();

        listener
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set non-blocking: {e}"))?;

        let host_ip = if bind_addr == "0.0.0.0" {
            get_local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "127.0.0.1".to_string())
        } else {
            bind_addr.to_string()
        };
        let base_url = format!("http://{host_ip}:{actual_port}");

        let state = Arc::new(build_serve_state(&storage_dir, &base_url)?);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let thread_handle = thread::spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        let state_clone = state.clone();
                        thread::spawn(move || {
                            let _ = handle_http_request(stream, state_clone);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            port: actual_port,
            running,
            thread_handle: Some(thread_handle),
        })
    }

    /// Returns the port the server is listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stops the server.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.thread_handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for BmsServeServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn handle_http_request(mut stream: TcpStream, state: Arc<ServeState>) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(15)));

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let method = parts[0];
    let raw_path = parts[1];
    let path = raw_path.split('?').next().unwrap_or(raw_path);

    // Consume request headers
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }

    if method != "GET" && method != "HEAD" {
        let resp = "HTTP/1.1 405 Method Not Allowed\r\nAllow: GET, HEAD\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(resp.as_bytes())?;
        return Ok(());
    }

    if path == "/" || path == "/index.json" {
        let body = state.index_json.as_bytes();
        let headers = format!(
            "HTTP/1.1 200 OK\r\n\
            Content-Type: application/json; charset=utf-8\r\n\
            Content-Length: {}\r\n\
            Access-Control-Allow-Origin: *\r\n\
            Connection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes())?;
        if method == "GET" {
            stream.write_all(body)?;
        }
        return Ok(());
    }

    if let Some(pkg_filename) = path.strip_prefix("/packages/") {
        // Path traversal defense
        if pkg_filename.contains("..") || pkg_filename.contains('/') || pkg_filename.contains('\\')
        {
            let resp = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(resp.as_bytes())?;
            return Ok(());
        }

        if let Some(target_path) = state.files.get(pkg_filename) {
            if let Ok(mut file) = File::open(target_path) {
                let size = fs::metadata(target_path)
                    .map(|m| m.len())
                    .unwrap_or_default();
                let headers = format!(
                    "HTTP/1.1 200 OK\r\n\
                    Content-Type: application/octet-stream\r\n\
                    Content-Length: {}\r\n\
                    Access-Control-Allow-Origin: *\r\n\
                    Connection: close\r\n\r\n",
                    size
                );
                stream.write_all(headers.as_bytes())?;
                if method == "GET" {
                    let mut buf = [0u8; 64 * 1024];
                    loop {
                        let n = file.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        stream.write_all(&buf[..n])?;
                    }
                }
                return Ok(());
            }
        }
    }

    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(resp.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bms_package::{Manifest, PackageBuilder};
    use std::time::SystemTime;

    fn create_test_package_on_disk(dir: &Path, id: &str, title: &str) -> PathBuf {
        let manifest = Manifest::new(id, title).with_author("Test Author");
        let mut builder = PackageBuilder::new(manifest);
        builder
            .add_file("bms/main.bms", b"#TITLE Test\n#BPM 140".to_vec())
            .unwrap();
        builder
            .add_file("audio/01.wav", vec![0x11, 0x22, 0x33, 0x44])
            .unwrap();
        let bytes = builder.build_to_bytes().unwrap();
        let path = dir.join(format!("{id}.bmsp"));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn test_compute_file_sha256() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_sha_test_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("test.txt");
        fs::write(&file_path, b"hello world").unwrap();

        let sha = compute_file_sha256(&file_path).unwrap();
        // SHA-256 of "hello world"
        assert_eq!(
            sha,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_bms_serve_server_index_and_package_streaming() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_serve_test_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // Put a test package in temp_dir
        let pkg_path = create_test_package_on_disk(&temp_dir, "org.test.song", "Test Song");
        let expected_sha = compute_file_sha256(&pkg_path).unwrap();
        let expected_size = fs::metadata(&pkg_path).unwrap().len();

        // Start server on ephemeral port (port 0)
        let server = BmsServeServer::start(temp_dir.clone(), "127.0.0.1", 0).unwrap();
        let port = server.port();
        assert!(port > 0);

        let client = crate::net::http::HttpClient::default();

        // 1. Fetch /index.json
        let index_url = format!("http://127.0.0.1:{port}/index.json");
        let index = client.fetch_index(&index_url).unwrap();
        assert_eq!(index.packages.len(), 1);
        let pkg = &index.packages[0];
        assert_eq!(pkg.id, "org.test.song");
        assert_eq!(pkg.title, "Test Song");
        assert_eq!(pkg.sha256, expected_sha);
        assert_eq!(pkg.size_bytes, expected_size);

        // 2. Download package via HTTP
        let download_dest = temp_dir.join("downloaded.bmsp");
        client
            .download_file_with_checksum(
                &pkg.download_url,
                &download_dest,
                &expected_sha,
                None,
                crate::net::http::NoopProgressCallback,
            )
            .unwrap();

        assert!(download_dest.exists());
        assert_eq!(fs::metadata(&download_dest).unwrap().len(), expected_size);
        assert_eq!(compute_file_sha256(&download_dest).unwrap(), expected_sha);

        // 3. Test 404 on nonexistent route
        let invalid_url = format!("http://127.0.0.1:{port}/nonexistent");
        assert!(client.fetch_index(&invalid_url).is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
