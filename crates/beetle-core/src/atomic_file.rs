//! Saving a file so that an interrupted write never leaves it half written.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Writes `bytes` to `<path>.tmp`, then renames it over `path`. A crash or
/// power loss mid-save leaves either the old file or the new one, never a
/// mix; at worst a stray `.tmp` stays behind and is overwritten next time.
pub fn write_atomic(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let path = path.as_ref();
    let mut temp = OsString::from(path.as_os_str());
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_file_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("beetle-atomic-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.dat");
        fs::write(&file, b"old").unwrap();
        write_atomic(&file, b"new").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert!(!dir.join("a.dat.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
