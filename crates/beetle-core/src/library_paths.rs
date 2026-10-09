//! The list of extra folders the player scans for existing BMS collections
//! (`library.dat`, managed by `bpm library`).
//!
//! Pure part only: parsing, editing and serializing. Checking that a folder
//! exists and turning it into an absolute path is the caller's job.

/// Folders to scan, in the order they were added. One path per line in the
/// file; blank lines and lines starting with `#` are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibraryPaths {
    paths: Vec<String>,
}

impl LibraryPaths {
    pub fn parse(text: &str) -> Self {
        let mut list = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                list.add(line);
            }
        }
        list
    }

    pub fn serialize(&self) -> String {
        let mut out =
            String::from("# BMS folders scanned by the player (bpm library add/remove)\n");
        for p in &self.paths {
            out.push_str(p);
            out.push('\n');
        }
        out
    }

    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Returns false when the path was already listed.
    pub fn add(&mut self, path: &str) -> bool {
        if self.paths.iter().any(|p| same_path(p, path)) {
            return false;
        }
        self.paths.push(path.to_string());
        true
    }

    /// Returns false when the path was not listed.
    pub fn remove(&mut self, path: &str) -> bool {
        let before = self.paths.len();
        self.paths.retain(|p| !same_path(p, path));
        self.paths.len() != before
    }
}

/// Windows paths ignore case and treat `/` like `\`.
fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.trim_end_matches(['/', '\\'])
            .replace('/', "\\")
            .to_lowercase()
    };
    norm(a) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skips_comments_blanks_and_duplicates() {
        let l = LibraryPaths::parse("# c\n\n D:\\BMS \nd:/bms/\nE:\\Other\n");
        assert_eq!(l.paths(), ["D:\\BMS", "E:\\Other"]);
    }

    #[test]
    fn add_remove_round_trip() {
        let mut l = LibraryPaths::default();
        assert!(l.add("D:\\BMS"));
        assert!(!l.add("d:\\bms\\"));
        assert!(l.add("E:\\x"));
        assert!(l.remove("D:/BMS"));
        assert!(!l.remove("D:\\BMS"));
        assert_eq!(LibraryPaths::parse(&l.serialize()), l);
    }
}

/// The folders scanned for charts, in scan order: the `library.dat` folders,
/// then the default songs folder, then the `BMS_DIR` folder. A folder listed
/// twice keeps its first place. The caller makes the paths absolute first.
pub fn collection_folders(
    library: &LibraryPaths,
    songs: Option<&str>,
    bms_dir: Option<&str>,
) -> Vec<String> {
    let mut folders: Vec<String> = Vec::new();
    let candidates = library
        .paths()
        .iter()
        .map(String::as_str)
        .chain(songs)
        .chain(bms_dir);
    for folder in candidates {
        if !folders.iter().any(|known| known == folder) {
            folders.push(folder.to_string());
        }
    }
    folders
}

#[cfg(test)]
mod collection_folder_tests {
    use super::*;

    #[test]
    fn folders_keep_library_order_then_songs_then_bms_dir() {
        let mut library = LibraryPaths::default();
        library.add("D:/Packs");
        library.add("E:/Old");
        assert_eq!(
            collection_folders(&library, Some("C:/game/songs"), Some("F:/Extra")),
            vec!["D:/Packs", "E:/Old", "C:/game/songs", "F:/Extra"]
        );
    }

    #[test]
    fn duplicate_folders_are_dropped_keeping_the_first() {
        let mut library = LibraryPaths::default();
        library.add("C:/game/songs");
        assert_eq!(
            collection_folders(&library, Some("C:/game/songs"), Some("C:/game/songs")),
            vec!["C:/game/songs"]
        );
    }
}
