use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use beetle_render::ImageBuffer;
use bms_package::{PackageReader, SoundAtlasCodec};

use crate::error::PackageManagerError;
use crate::export::extract_wav_from_atlas;

/// Virtual file representation in the virtual file system.
#[derive(Clone)]
pub enum VirtualFile {
    /// In-memory byte buffer (e.g. metadata, generated text).
    Memory(Arc<[u8]>),
    /// Archive entry in a .bmsp file read on-demand.
    ArchiveEntry {
        package_path: PathBuf,
        entry_path: String,
    },
    /// On-the-fly synthesized 16-bit PCM RIFF WAV from Sound Atlas slice.
    SynthesizedWav {
        atlas_bytes: Arc<[u8]>,
        codec: SoundAtlasCodec,
        sample_rate: u32,
        start_frame: u64,
        frame_count: u64,
    },
    /// Pre-cropped BMP image from BGA Atlas.
    SynthesizedBmp(Arc<[u8]>),
}

impl VirtualFile {
    pub fn len(&self) -> usize {
        match self {
            Self::Memory(b) => b.len(),
            Self::SynthesizedBmp(b) => b.len(),
            Self::SynthesizedWav {
                codec, frame_count, ..
            } => {
                let pcm_len = match codec {
                    SoundAtlasCodec::Pcm16 => (*frame_count as usize) * 4,
                    SoundAtlasCodec::PcmF32 => (*frame_count as usize) * 4, // converted to 16-bit
                    SoundAtlasCodec::OggBundle | SoundAtlasCodec::WavBundle => {
                        *frame_count as usize
                    }
                };
                44 + pcm_len
            }
            Self::ArchiveEntry {
                package_path,
                entry_path,
            } => {
                if let Ok(mut pkg) = PackageReader::open_file(package_path) {
                    if let Ok(data) = pkg.read_entry(entry_path) {
                        return data.len();
                    }
                }
                0
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read(&self) -> Option<Vec<u8>> {
        match self {
            Self::Memory(b) => Some(b.to_vec()),
            Self::SynthesizedBmp(b) => Some(b.to_vec()),
            Self::SynthesizedWav {
                atlas_bytes,
                codec,
                sample_rate,
                start_frame,
                frame_count,
            } => extract_wav_from_atlas(
                *codec,
                *sample_rate,
                atlas_bytes,
                *start_frame,
                *frame_count,
            ),
            Self::ArchiveEntry {
                package_path,
                entry_path,
            } => {
                let mut pkg = PackageReader::open_file(package_path).ok()?;
                pkg.read_entry(entry_path).ok()
            }
        }
    }
}

/// Directory entry description in the virtual file system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VfsEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: usize,
}

/// Tree node in the Virtual BMS File System.
enum VfsNode {
    Directory { children: HashMap<String, VfsNode> },
    File(VirtualFile),
}

/// Virtual BMS File System (VFS) that maps multiple `.bmsp` packages into
/// a unified directory tree, synthesizing WAV and BMP files on-the-fly.
pub struct VirtualBmsFs {
    root: VfsNode,
}

impl VirtualBmsFs {
    pub fn new() -> Self {
        Self {
            root: VfsNode::Directory {
                children: HashMap::new(),
            },
        }
    }

    /// Mounts an individual `.bmsp` package under a specified virtual mount folder name.
    pub fn mount_package<P: AsRef<Path>>(
        &mut self,
        mount_name: &str,
        package_path: P,
    ) -> Result<(), PackageManagerError> {
        let pkg_path = package_path.as_ref().to_path_buf();
        let mut pkg = PackageReader::open_file(&pkg_path)?;
        let manifest = pkg.manifest().clone();

        let mut song_dir = HashMap::new();

        let is_turbo = manifest.sound_atlas.is_some() || manifest.bga_atlas.is_some();

        if is_turbo {
            // 1. Mount Sound Atlas slices as on-the-fly WAV virtual files
            if let Some(ref sound_meta) = manifest.sound_atlas {
                if let Ok(atlas_bytes) = pkg.read_entry(&sound_meta.file) {
                    let atlas_arc: Arc<[u8]> = Arc::from(atlas_bytes.into_boxed_slice());
                    if sound_meta.codec.is_bundle() {
                        let default_ext = if sound_meta.codec == SoundAtlasCodec::OggBundle {
                            ".ogg"
                        } else {
                            ".wav"
                        };
                        for (key, slice) in &sound_meta.slices {
                            let filename = slice.original_filename.as_deref().unwrap_or(key);

                            let filename = if !filename.to_lowercase().ends_with(".wav")
                                && !filename.to_lowercase().ends_with(".ogg")
                            {
                                format!("{}{}", filename, default_ext)
                            } else {
                                filename.to_string()
                            };

                            let start = slice.start_frame as usize;
                            let len = slice.frame_count as usize;
                            if start + len <= atlas_arc.len() {
                                let vfile =
                                    VirtualFile::Memory(Arc::from(&atlas_arc[start..start + len]));
                                song_dir.insert(filename, VfsNode::File(vfile));
                            }
                        }
                    } else {
                        for (key, slice) in &sound_meta.slices {
                            let filename = slice.original_filename.as_deref().unwrap_or(key);

                            let filename = if !filename.to_lowercase().ends_with(".wav")
                                && !filename.to_lowercase().ends_with(".ogg")
                            {
                                format!("{}.wav", filename)
                            } else {
                                filename.to_string()
                            };

                            let vfile = VirtualFile::SynthesizedWav {
                                atlas_bytes: Arc::clone(&atlas_arc),
                                codec: sound_meta.codec,
                                sample_rate: sound_meta.sample_rate,
                                start_frame: slice.start_frame,
                                frame_count: slice.frame_count,
                            };

                            song_dir.insert(filename, VfsNode::File(vfile));
                        }
                    }
                }
            }

            // 2. Mount BGA Atlas frames as BMP virtual files
            if let Some(ref bga_meta) = manifest.bga_atlas {
                if let Ok(atlas_bytes) = pkg.read_entry(&bga_meta.file) {
                    if let Some(atlas_img) = ImageBuffer::from_bytes(&atlas_bytes) {
                        for (key, frame) in &bga_meta.frames {
                            let filename = frame.original_filename.as_deref().unwrap_or(key);

                            let filename = if !filename.to_lowercase().ends_with(".bmp")
                                && !filename.to_lowercase().ends_with(".png")
                                && !filename.to_lowercase().ends_with(".jpg")
                            {
                                format!("{}.bmp", filename)
                            } else {
                                filename.to_string()
                            };

                            if let Some(sub_img) =
                                atlas_img.crop(frame.x, frame.y, frame.width, frame.height)
                            {
                                let bmp_bytes = sub_img.encode_bmp_bytes();
                                let vfile = VirtualFile::SynthesizedBmp(Arc::from(
                                    bmp_bytes.into_boxed_slice(),
                                ));
                                song_dir.insert(filename, VfsNode::File(vfile));
                            }
                        }
                    }
                }
            }

            // 3. Mount non-atlas entries (charts, videos, text)
            let entries: Vec<String> = pkg.entries().iter().map(|e| e.path.clone()).collect();
            for entry_path in entries {
                if entry_path == bms_package::MANIFEST_FILENAME {
                    continue;
                }
                if let Some(ref sm) = manifest.sound_atlas {
                    if entry_path == sm.file {
                        continue;
                    }
                }
                if let Some(ref bm) = manifest.bga_atlas {
                    if entry_path == bm.file {
                        continue;
                    }
                }

                let vfile = VirtualFile::ArchiveEntry {
                    package_path: pkg_path.clone(),
                    entry_path: entry_path.clone(),
                };
                song_dir.insert(entry_path, VfsNode::File(vfile));
            }
        } else {
            // Classic package: mount all archive entries directly
            let entries: Vec<String> = pkg.entries().iter().map(|e| e.path.clone()).collect();
            for entry_path in entries {
                if entry_path == bms_package::MANIFEST_FILENAME {
                    continue;
                }
                let vfile = VirtualFile::ArchiveEntry {
                    package_path: pkg_path.clone(),
                    entry_path: entry_path.clone(),
                };
                song_dir.insert(entry_path, VfsNode::File(vfile));
            }
        }

        if let VfsNode::Directory { ref mut children } = self.root {
            match children.entry(mount_name.to_string()) {
                std::collections::hash_map::Entry::Occupied(mut occ) => {
                    if let VfsNode::Directory {
                        children: ref mut existing,
                    } = occ.get_mut()
                    {
                        existing.extend(song_dir);
                    }
                }
                std::collections::hash_map::Entry::Vacant(vac) => {
                    vac.insert(VfsNode::Directory { children: song_dir });
                }
            }
        }

        Ok(())
    }

    /// Mounts all packages in a directory (e.g. `packages/`), pairing base packages and BGA companions.
    pub fn mount_directory<P: AsRef<Path>>(
        &mut self,
        dir_path: P,
    ) -> Result<usize, PackageManagerError> {
        let mut count = 0;
        let dir = dir_path.as_ref();
        if dir.exists() {
            let mut base_packages = Vec::new();
            let mut companion_packages = Vec::new();

            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let p = entry.path();
                if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("bmsp") {
                    let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if file_name.ends_with(".bga.bmsp") {
                        companion_packages.push(p);
                    } else {
                        base_packages.push(p);
                    }
                }
            }

            // 1. Mount base packages first
            for p in base_packages {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("song");
                if self.mount_package(stem, &p).is_ok() {
                    count += 1;
                }
            }

            // 2. Mount and overlay companion packages onto their target directories
            for p in companion_packages {
                let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let mut target_mount = file_name
                    .strip_suffix(".bga.bmsp")
                    .unwrap_or("")
                    .to_string();

                // If possible, read companion manifest to obtain target_package_id
                if let Ok(pkg) = PackageReader::open_file(&p) {
                    if let Some(ref target_id) = pkg.manifest().target_package_id {
                        if !target_id.is_empty() {
                            if let VfsNode::Directory { ref children } = self.root {
                                if children.contains_key(target_id) {
                                    target_mount = target_id.clone();
                                }
                            }
                        }
                    }
                }

                if target_mount.is_empty() {
                    target_mount = p
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("bga")
                        .to_string();
                }

                if self.mount_package(&target_mount, &p).is_ok() {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    fn normalize_path(path: &str) -> Vec<String> {
        let trimmed = path.trim_matches('/').replace('\\', "/");
        trimmed
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".")
            .map(|s| s.to_string())
            .collect()
    }

    fn navigate(&self, parts: &[String]) -> Option<&VfsNode> {
        let mut curr = &self.root;
        let mut i = 0;
        while i < parts.len() {
            match curr {
                VfsNode::Directory { children } => {
                    // Check if the remaining path joined with '/' matches a direct entry
                    let remainder = parts[i..].join("/");
                    if let Some(node) = children.get(&remainder) {
                        return Some(node);
                    }
                    // Otherwise navigate to next segment
                    curr = children.get(&parts[i])?;
                    i += 1;
                }
                VfsNode::File(_) => return None,
            }
        }
        Some(curr)
    }

    pub fn list_dir(&self, path: &str) -> Option<Vec<VfsEntry>> {
        let parts = Self::normalize_path(path);
        let node = self.navigate(&parts)?;

        match node {
            VfsNode::Directory { children } => {
                let mut entries = Vec::new();
                for (name, child) in children {
                    match child {
                        VfsNode::Directory { .. } => {
                            entries.push(VfsEntry {
                                name: name.clone(),
                                is_dir: true,
                                size: 0,
                            });
                        }
                        VfsNode::File(f) => {
                            entries.push(VfsEntry {
                                name: name.clone(),
                                is_dir: false,
                                size: f.len(),
                            });
                        }
                    }
                }
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                Some(entries)
            }
            VfsNode::File(_) => None,
        }
    }

    pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        let parts = Self::normalize_path(path);
        let node = self.navigate(&parts)?;
        match node {
            VfsNode::File(f) => f.read(),
            VfsNode::Directory { .. } => None,
        }
    }

    pub fn file_size(&self, path: &str) -> Option<usize> {
        let parts = Self::normalize_path(path);
        let node = self.navigate(&parts)?;
        match node {
            VfsNode::File(f) => Some(f.len()),
            VfsNode::Directory { .. } => None,
        }
    }

    pub fn is_dir(&self, path: &str) -> bool {
        let parts = Self::normalize_path(path);
        match self.navigate(&parts) {
            Some(VfsNode::Directory { .. }) => true,
            _ => false,
        }
    }
}

impl Default for VirtualBmsFs {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight WebDAV / HTTP server that exposes VirtualBmsFs as a virtual network drive.
pub struct WebDavServer {
    port: u16,
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl WebDavServer {
    /// Starts the WebDAV server on the specified port. If port is 0, an ephemeral port is assigned by OS.
    pub fn start(fs: Arc<VirtualBmsFs>, port: u16) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
        let bound_port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        let thread_handle = thread::spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let fs_ref = Arc::clone(&fs);
                        thread::spawn(move || {
                            let _ = handle_client(stream, fs_ref);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            port: bound_port,
            running,
            thread_handle: Some(thread_handle),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.thread_handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for WebDavServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn handle_client(mut stream: TcpStream, fs: Arc<VirtualBmsFs>) -> std::io::Result<()> {
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

    // Consume headers
    let mut depth = "infinity".to_string();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if line.to_lowercase().starts_with("depth:") {
            depth = line[6..].trim().to_lowercase();
        }
    }

    match method {
        "OPTIONS" => {
            let response = "HTTP/1.1 200 OK\r\n\
                DAV: 1, 2\r\n\
                MS-Author-Via: DAV\r\n\
                Allow: GET, HEAD, OPTIONS, PROPFIND\r\n\
                Content-Length: 0\r\n\
                Connection: close\r\n\r\n";
            stream.write_all(response.as_bytes())?;
        }
        "PROPFIND" => {
            handle_propfind(&mut stream, &fs, path, &depth)?;
        }
        "GET" => {
            if let Some(content) = fs.read_file(path) {
                let mime = guess_mime_type(path);
                let header = format!(
                    "HTTP/1.1 200 OK\r\n\
                    Content-Type: {}\r\n\
                    Content-Length: {}\r\n\
                    Connection: close\r\n\r\n",
                    mime,
                    content.len()
                );
                stream.write_all(header.as_bytes())?;
                stream.write_all(&content)?;
            } else if fs.is_dir(path) {
                handle_propfind(&mut stream, &fs, path, "1")?;
            } else {
                let response =
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(response.as_bytes())?;
            }
        }
        "HEAD" => {
            if let Some(size) = fs.file_size(path) {
                let mime = guess_mime_type(path);
                let header = format!(
                    "HTTP/1.1 200 OK\r\n\
                    Content-Type: {}\r\n\
                    Content-Length: {}\r\n\
                    Connection: close\r\n\r\n",
                    mime, size
                );
                stream.write_all(header.as_bytes())?;
            } else if fs.is_dir(path) {
                let header = "HTTP/1.1 200 OK\r\nContent-Type: httpd/unix-directory\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(header.as_bytes())?;
            } else {
                let response =
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(response.as_bytes())?;
            }
        }
        _ => {
            let response = "HTTP/1.1 405 Method Not Allowed\r\nAllow: GET, HEAD, OPTIONS, PROPFIND\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(response.as_bytes())?;
        }
    }

    stream.flush()?;
    Ok(())
}

fn handle_propfind(
    stream: &mut TcpStream,
    fs: &VirtualBmsFs,
    path: &str,
    depth: &str,
) -> std::io::Result<()> {
    let normalized = path.trim_end_matches('/');
    let target_path = if normalized.is_empty() {
        "/"
    } else {
        normalized
    };

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\" ?>\r\n<D:multistatus xmlns:D=\"DAV:\">\r\n",
    );

    if fs.is_dir(target_path) {
        // Target directory response
        let href = if target_path == "/" {
            "/".to_string()
        } else {
            format!("{}/", target_path)
        };
        xml.push_str(&format!(
            "  <D:response>\r\n\
            \x20   <D:href>{}</D:href>\r\n\
            \x20   <D:propstat>\r\n\
            \x20     <D:prop>\r\n\
            \x20       <D:resourcetype><D:collection/></D:resourcetype>\r\n\
            \x20     </D:prop>\r\n\
            \x20     <D:status>HTTP/1.1 200 OK</D:status>\r\n\
            \x20   </D:propstat>\r\n\
            \x20 </D:response>\r\n",
            href
        ));

        // If Depth != "0", list directory children
        if depth != "0" {
            if let Some(entries) = fs.list_dir(target_path) {
                for entry in entries {
                    let child_href = if target_path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", target_path, entry.name)
                    };

                    let resourcetype = if entry.is_dir { "<D:collection/>" } else { "" };

                    xml.push_str(&format!(
                        "  <D:response>\r\n\
                        \x20   <D:href>{}</D:href>\r\n\
                        \x20   <D:propstat>\r\n\
                        \x20     <D:prop>\r\n\
                        \x20       <D:displayname>{}</D:displayname>\r\n\
                        \x20       <D:getcontentlength>{}</D:getcontentlength>\r\n\
                        \x20       <D:resourcetype>{}</D:resourcetype>\r\n\
                        \x20     </D:prop>\r\n\
                        \x20     <D:status>HTTP/1.1 200 OK</D:status>\r\n\
                        \x20   </D:propstat>\r\n\
                        \x20 </D:response>\r\n",
                        child_href, entry.name, entry.size, resourcetype
                    ));
                }
            }
        }
    } else if let Some(size) = fs.file_size(target_path) {
        let filename = target_path.rsplit('/').next().unwrap_or(target_path);
        xml.push_str(&format!(
            "  <D:response>\r\n\
            \x20   <D:href>{}</D:href>\r\n\
            \x20   <D:propstat>\r\n\
            \x20     <D:prop>\r\n\
            \x20       <D:displayname>{}</D:displayname>\r\n\
            \x20       <D:getcontentlength>{}</D:getcontentlength>\r\n\
            \x20       <D:resourcetype/>\r\n\
            \x20     </D:prop>\r\n\
            \x20     <D:status>HTTP/1.1 200 OK</D:status>\r\n\
            \x20   </D:propstat>\r\n\
            \x20 </D:response>\r\n",
            target_path, filename, size
        ));
    } else {
        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    xml.push_str("</D:multistatus>\r\n");

    let header = format!(
        "HTTP/1.1 207 Multi-Status\r\n\
        Content-Type: text/xml; charset=\"utf-8\"\r\n\
        Content-Length: {}\r\n\
        Connection: close\r\n\r\n",
        xml.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(xml.as_bytes())?;
    Ok(())
}

fn guess_mime_type(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "bmp" => "image/bmp",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "bms" | "bme" | "bml" | "pms" => "text/plain; charset=utf-8",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_render::skin::ColorRgba;

    #[test]
    fn test_vfs_mount_and_synthesized_wav_streaming() {
        let temp_src = std::env::temp_dir().join(format!(
            "bpm_vfs_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_src).unwrap();

        // 1. Create chart, wav, and bmp
        fs::write(
            temp_src.join("track.bms"),
            "#TITLE VFS Test\n#WAV01 kick.wav\n#BMP01 bg.bmp\n",
        )
        .unwrap();

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
            w.write_sample(500i16).unwrap();
            w.write_sample(-500i16).unwrap();
            w.finalize().unwrap();
        }
        fs::write(temp_src.join("kick.wav"), cur.into_inner()).unwrap();

        let img = ImageBuffer::new(8, 8, ColorRgba::new(0, 255, 128, 255));
        fs::write(temp_src.join("bg.bmp"), img.encode_bmp_bytes()).unwrap();

        // 2. Pack as Turbo package
        let pkg_bytes =
            crate::pack_bms_folder_profile(&temp_src, None, crate::PackProfile::Turbo).unwrap();
        let pkg_path = temp_src.join("vfs_song.bmsp");
        fs::write(&pkg_path, pkg_bytes).unwrap();

        // 3. Mount in VirtualBmsFs
        let mut vfs = VirtualBmsFs::new();
        vfs.mount_package("vfs_song", &pkg_path)
            .expect("mount failed");

        // 4. Verify virtual directory structure
        let root_entries = vfs.list_dir("/").expect("list root");
        assert_eq!(root_entries.len(), 1);
        assert_eq!(root_entries[0].name, "vfs_song");
        assert!(root_entries[0].is_dir);

        let song_entries = vfs.list_dir("/vfs_song").expect("list song dir");
        let names: Vec<String> = song_entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains(&"track.bms".to_string()));
        assert!(names.contains(&"kick.wav".to_string()));
        assert!(names.contains(&"bg.bmp".to_string()));

        // 5. Read on-the-fly synthesized WAV file
        let wav_data = vfs.read_file("/vfs_song/kick.wav").expect("read kick.wav");
        assert!(wav_data.len() > 44);
        assert_eq!(&wav_data[0..4], b"RIFF");
        assert_eq!(&wav_data[8..12], b"WAVE");

        // Verify with hound decoder
        let wav_reader = hound::WavReader::new(std::io::Cursor::new(wav_data)).unwrap();
        assert_eq!(wav_reader.spec().channels, 1);
        assert_eq!(wav_reader.spec().sample_rate, 44100);

        // 6. Read on-the-fly synthesized BMP file
        let bmp_data = vfs.read_file("/vfs_song/bg.bmp").expect("read bg.bmp");
        let loaded_img = ImageBuffer::from_bytes(&bmp_data).expect("valid BMP");
        assert_eq!(loaded_img.width, 8);
        assert_eq!(loaded_img.height, 8);

        // 7. Test WebDAV Server HTTP loopback
        let vfs_arc = Arc::new(vfs);
        let server = WebDavServer::start(vfs_arc, 0).expect("start server");
        let port = server.port();

        // Test GET /vfs_song/kick.wav via raw TCP stream
        let mut stream =
            TcpStream::connect(format!("127.0.0.1:{}", port)).expect("connect to webdav");
        stream
            .write_all(b"GET /vfs_song/kick.wav HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(
            status_line.contains("200 OK"),
            "Expected 200 OK, got: {}",
            status_line
        );

        // Clean up
        let _ = fs::remove_dir_all(&temp_src);
    }

    #[test]
    fn test_vfs_bga_companion_pairing_and_overlay() {
        use bms_package::{Manifest, PackageBuilder};

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_vfs_companion_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Create base package (song.bmsp)
        let base_manifest = Manifest::new("com.example.paired", "Paired Song");
        let mut base_builder = PackageBuilder::new(base_manifest);
        base_builder
            .add_file("main.bms", b"#TITLE Paired Song\n#BMP01 movie.mp4".to_vec())
            .unwrap();
        base_builder
            .add_file("audio/01.wav", vec![1, 2, 3, 4])
            .unwrap();
        let base_bytes = base_builder.build_to_bytes().unwrap();
        fs::write(temp_dir.join("paired_song.bmsp"), base_bytes).unwrap();

        // 2. Create companion package (song.bga.bmsp)
        let bga_manifest = Manifest::new_bga_companion(
            "com.example.paired_bga",
            "Paired Song (BGA)",
            "com.example.paired",
        );
        let mut bga_builder = PackageBuilder::new(bga_manifest);
        bga_builder
            .add_file("visual/movie.mp4", vec![0x99, 0x88, 0x77, 0x66])
            .unwrap();
        let bga_bytes = bga_builder.build_to_bytes().unwrap();
        fs::write(temp_dir.join("paired_song.bga.bmsp"), bga_bytes).unwrap();

        // 3. Mount directory with VirtualBmsFs
        let mut vfs = VirtualBmsFs::new();
        let count = vfs
            .mount_directory(&temp_dir)
            .expect("mount directory failed");
        assert_eq!(count, 2);

        // 4. Verify that paired_song contains both base files and BGA companion files
        let entries = vfs.list_dir("/paired_song").expect("list paired_song");
        let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains(&"main.bms".to_string()));
        assert!(names.contains(&"audio/01.wav".to_string()));
        assert!(names.contains(&"visual/movie.mp4".to_string()));

        // 5. Read BGA companion file from the unified mount
        let video_data = vfs
            .read_file("/paired_song/visual/movie.mp4")
            .expect("read movie.mp4");
        assert_eq!(video_data, vec![0x99, 0x88, 0x77, 0x66]);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
