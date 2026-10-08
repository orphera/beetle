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
        let mut out = String::from("# BMS folders scanned by the player (bpm library add/remove)\n");
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
    let norm = |s: &str| s.trim_end_matches(['/', '\\']).replace('/', "\\").to_lowercase();
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
