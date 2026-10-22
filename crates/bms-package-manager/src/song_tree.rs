//! Where the packer reads a song's files from: a folder on disk, or a folder
//! inside a zip read in place (no unpacking). The packer only lists files and
//! reads their bytes, so both look the same to it.

use crate::archive::safe_relative_path;
use crate::PackageManagerError;
use bms_package::MANIFEST_FILENAME;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One song's files.
pub trait SongTree {
    /// The song's folder name (the manifest's fallback title).
    fn name(&self) -> String;
    /// Every file of the song, as paths relative to its folder with `/`,
    /// sorted. Manifests, `.bmsp` files and dot files are left out.
    fn files(&self) -> Vec<String>;
    /// The bytes of `rel`, one of `files()`.
    fn read(&self, rel: &str) -> io::Result<Vec<u8>>;
}

const CHART_EXTENSIONS: [&str; 4] = ["bms", "bme", "bml", "pms"];

pub(crate) fn is_chart(rel: &str) -> bool {
    rel.rsplit_once('.')
        .is_some_and(|(_, ext)| CHART_EXTENSIONS.iter().any(|c| ext.eq_ignore_ascii_case(c)))
}

/// Files the packer skips wherever they are.
fn skipped(file_name: &str) -> bool {
    file_name == MANIFEST_FILENAME || file_name.ends_with(".bmsp") || file_name.starts_with('.')
}

/// A song folder on disk.
pub struct DirSong {
    root: PathBuf,
}

impl DirSong {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }
}

impl SongTree for DirSong {
    fn name(&self) -> String {
        self.root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("bms_song")
            .to_string()
    }

    fn files(&self) -> Vec<String> {
        fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else if path.is_file() {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if skipped(file_name) {
                        continue;
                    }
                    if let Ok(rel) = path.strip_prefix(base) {
                        out.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        out.sort();
        out
    }

    fn read(&self, rel: &str) -> io::Result<Vec<u8>> {
        fs::read(self.root.join(rel))
    }
}

/// A zip opened in place: its file entries by safe relative path.
pub struct ZipTree {
    archive: Mutex<zip::ZipArchive<File>>,
    /// (relative path with `/`, entry index), sorted by path.
    entries: Vec<(String, usize)>,
}

impl ZipTree {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PackageManagerError> {
        let file = File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| PackageManagerError::InvalidPackage(format!("not a valid zip: {e}")))?;
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index).map_err(|e| {
                PackageManagerError::InvalidPackage(format!("corrupt zip entry: {e}"))
            })?;
            if entry.is_dir() {
                continue;
            }
            // Names decoded as the extractor decodes them (CP949 / Shift-JIS zips).
            let name = beetle_core::decode_bms_text(entry.name_raw());
            let Some(rel) = safe_relative_path(&name) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            entries.push((rel, index));
        }
        entries.sort();
        entries.dedup_by(|a, b| a.0 == b.0);
        Ok(Self {
            archive: Mutex::new(archive),
            entries,
        })
    }

    /// The song folders in the zip, as the folder search finds them on disk:
    /// the top level if it holds charts, else the shallowest folders that do
    /// (folders starting with `.` are not looked into).
    pub fn song_dirs(&self) -> Vec<String> {
        song_dirs(self.entries.iter().map(|(rel, _)| rel.as_str()))
    }

    /// The song in folder `dir` (from `song_dirs`), named `name`.
    pub fn song<'a>(&'a self, dir: &str, name: &str) -> ZipSong<'a> {
        ZipSong {
            zip: self,
            prefix: if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            },
            name: name.to_string(),
        }
    }
}

/// The song folders among `files` (relative paths with `/`); see `ZipTree::song_dirs`.
pub(crate) fn song_dirs<'a>(files: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut chart_dirs: Vec<String> = files
        .filter(|f| is_chart(f))
        .map(|f| f.rsplit_once('/').map_or("", |(d, _)| d).to_string())
        .collect();
    chart_dirs.sort();
    chart_dirs.dedup();
    if chart_dirs.iter().any(|d| d.is_empty()) {
        return vec![String::new()];
    }
    let hidden = |d: &str| d.split('/').any(|part| part.starts_with('.'));
    let mut roots: Vec<String> = Vec::new();
    for dir in chart_dirs.iter().filter(|d| !hidden(d)) {
        // A folder under a song folder belongs to that song.
        let inside = chart_dirs
            .iter()
            .any(|other| other != dir && dir.starts_with(&format!("{other}/")) && !hidden(other));
        if !inside {
            roots.push(dir.clone());
        }
    }
    roots
}

/// One song folder of a `ZipTree`.
pub struct ZipSong<'a> {
    zip: &'a ZipTree,
    /// The folder with a trailing `/`, or empty for the top level.
    prefix: String,
    name: String,
}

impl SongTree for ZipSong<'_> {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn files(&self) -> Vec<String> {
        self.zip
            .entries
            .iter()
            .filter_map(|(rel, _)| rel.strip_prefix(&self.prefix))
            .filter(|rel| {
                let file_name = rel.rsplit('/').next().unwrap_or(rel);
                !skipped(file_name)
            })
            .map(str::to_string)
            .collect()
    }

    fn read(&self, rel: &str) -> io::Result<Vec<u8>> {
        let full = format!("{}{rel}", self.prefix);
        let index = self
            .zip
            .entries
            .binary_search_by(|(r, _)| r.as_str().cmp(&full))
            .map(|i| self.zip.entries[i].1)
            .map_err(|_| io::Error::new(io::ErrorKind::NotFound, full.clone()))?;
        let mut archive = self
            .zip
            .archive
            .lock()
            .map_err(|_| io::Error::other("zip reader poisoned"))?;
        let mut entry = archive.by_index(index).map_err(io::Error::other)?;
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_dirs_follow_the_folder_search() {
        let top = ["a.bms", "snd/a.wav", "Other/b.bms"];
        assert_eq!(song_dirs(top.into_iter()), [""]);
        let nested = [
            "Pack/One/x.bms",
            "Pack/One/extra/y.bms",
            "Pack/Two/z.bme",
            ".hidden/w.bms",
            "readme.txt",
        ];
        assert_eq!(song_dirs(nested.into_iter()), ["Pack/One", "Pack/Two"]);
    }
}
