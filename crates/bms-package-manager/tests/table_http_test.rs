//! `bpm table` against a real HTTP server on the loopback interface: redirects,
//! error statuses, the size cap, and a whole table fetched from its page.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use bms_package_manager::{fetch_table, HttpClient, TableStore};

const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn response(status: &str, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status}\r\nConnection: close\r\n").into_bytes();
    for (key, value) in headers {
        out.extend_from_slice(format!("{key}: {value}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    out.extend_from_slice(body);
    out
}

fn route(path: &str) -> Vec<u8> {
    match path {
        "/table.html" => response(
            "200 OK",
            &[("Content-Type", "text/html".into())],
            br#"<html><head><meta name="bmstable" content="data/header.json"></head></html>"#,
        ),
        "/data/header.json" => response(
            "200 OK",
            &[("Content-Type", "application/json".into())],
            br#"{"name":"Loopback","symbol":"lb","data_url":"../score.json"}"#,
        ),
        "/score.json" => response(
            "200 OK",
            &[],
            format!(r#"[{{"sha256":"{SHA}","level":"1","title":"One"}},{{"sha256":"{SHA}","level":"2","title":"Two"}}]"#).as_bytes(),
        ),
        "/old-place" => response("302 Found", &[("Location", "/score.json".into())], b""),
        "/missing" => response("404 Not Found", &[], b"nope"),
        "/big" => response("200 OK", &[], &vec![b'x'; 100]),
        // Says nothing about its size; the cap has to stop it anyway.
        "/stream" => {
            let mut out = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
            out.extend(std::iter::repeat_n(b'y', 5000));
            out
        }
        _ => response("404 Not Found", &[], b""),
    }
}

fn handle(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => request.extend_from_slice(&buffer[..n]),
        }
    }
    let text = String::from_utf8_lossy(&request);
    let path = text.split_whitespace().nth(1).unwrap_or("/");
    let _ = stream.write_all(&route(path));
}

/// Serves `route` until dropped.
struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        thread::spawn(move || handle(stream));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self { port, stop }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn client() -> HttpClient {
    HttpClient::with_timeouts(Duration::from_secs(3), Duration::from_secs(5))
}

#[test]
fn small_documents_come_back_whole_and_redirects_are_followed() {
    let server = Server::start();
    let client = client();
    let direct = client.get_bytes(&server.url("/score.json"), 1_000_000).unwrap();
    assert!(direct.starts_with(b"[{"));
    let redirected = client.get_bytes(&server.url("/old-place"), 1_000_000).unwrap();
    assert_eq!(redirected, direct);
}

#[test]
fn error_statuses_and_oversized_bodies_are_refused() {
    let server = Server::start();
    let client = client();
    assert!(client.get_bytes(&server.url("/missing"), 1_000_000).is_err());
    assert!(client.get_bytes("http://127.0.0.1:1/nothing-listens-here", 1_000_000).is_err());

    // Announced too large: refused before reading.
    let announced = client.get_bytes(&server.url("/big"), 50).unwrap_err();
    assert!(announced.contains("100 bytes"), "{announced}");
    // Not announced: refused once it goes past the cap.
    let streamed = client.get_bytes(&server.url("/stream"), 1000).unwrap_err();
    assert!(streamed.contains("more than"), "{streamed}");
    // And within the cap it is fine.
    assert_eq!(client.get_bytes(&server.url("/big"), 100).unwrap().len(), 100);
}

#[test]
fn a_whole_table_is_fetched_and_installed_over_http() {
    let server = Server::start();
    let client = client();
    let get = |url: &str, max: u64| client.get_bytes(url, max);

    let table = fetch_table(&get, &server.url("/table.html")).unwrap();
    assert_eq!((table.name.as_str(), table.symbol.as_str()), ("Loopback", "lb"));
    assert_eq!(table.source, server.url("/data/header.json"));
    assert_eq!(table.entries.len(), 2);

    let dir = std::env::temp_dir().join(format!("bpm_table_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = TableStore::new(&dir);
    store.install(&table).unwrap();
    let installed = store.find("loopback").unwrap();
    assert_eq!(installed.1.entries.len(), 2);

    // A table that serves nothing usable is refused, and what is installed stays.
    assert!(fetch_table(&get, &server.url("/missing")).is_err());
    assert_eq!(store.find("loopback").unwrap().1.entries.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}
