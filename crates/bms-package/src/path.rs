use crate::error::PackageError;

/// Validates that an entry path satisfies all security, normalization, and portability rules.
///
/// Rules:
/// - Must not be empty.
/// - Must use forward slashes `/` as separators.
/// - Backslashes `\` are strictly rejected.
/// - Absolute paths (`/foo`, `C:\foo`, `D:/foo`) are strictly rejected.
/// - Path traversal segments (`..`) are strictly rejected.
/// - Relative self-referential segments (`.`) are strictly rejected.
/// - Empty segments (`//`) or leading/trailing slashes are rejected.
/// - Control characters (< 0x20 or 0x7F) are rejected.
pub fn validate_entry_path(path: &str) -> Result<(), PackageError> {
    if path.is_empty() {
        return Err(PackageError::InvalidEntryPath("Path cannot be empty".to_string()));
    }

    // Reject backslashes completely
    if path.contains('\\') {
        return Err(PackageError::InvalidEntryPath(format!(
            "Path '{path}' contains backslashes '\\' (only '/' is allowed)"
        )));
    }

    // Reject leading slash (absolute Unix path)
    if path.starts_with('/') {
        return Err(PackageError::InvalidEntryPath(format!(
            "Path '{path}' cannot start with a leading slash '/'"
        )));
    }

    // Reject trailing slash
    if path.ends_with('/') {
        return Err(PackageError::InvalidEntryPath(format!(
            "Path '{path}' cannot end with a trailing slash '/'"
        )));
    }

    // Reject Windows drive prefix (e.g. C:, D:)
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(PackageError::InvalidEntryPath(format!(
            "Path '{path}' contains a Windows drive letter prefix"
        )));
    }

    // Check individual segments
    for segment in path.split('/') {
        if segment.is_empty() {
            return Err(PackageError::InvalidEntryPath(format!(
                "Path '{path}' contains an empty segment '//'"
            )));
        }

        if segment == ".." {
            return Err(PackageError::InvalidEntryPath(format!(
                "Path '{path}' contains illegal traversal segment '..'"
            )));
        }

        if segment == "." {
            return Err(PackageError::InvalidEntryPath(format!(
                "Path '{path}' contains redundant segment '.'"
            )));
        }

        // Check for ASCII control characters
        for &b in segment.as_bytes() {
            if b < 0x20 || b == 0x7F {
                return Err(PackageError::InvalidEntryPath(format!(
                    "Path '{path}' contains non-printable control characters"
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_paths() {
        assert!(validate_entry_path("manifest.json").is_ok());
        assert!(validate_entry_path("bms/main.bms").is_ok());
        assert!(validate_entry_path("audio/01.wav").is_ok());
        assert!(validate_entry_path("image/bg/stage.png").is_ok());
        assert!(validate_entry_path("folder/subfolder/file-name_123.ogg").is_ok());
        assert!(validate_entry_path("bms/동방곡.bms").is_ok());
    }

    #[test]
    fn test_invalid_paths_security() {
        // Empty
        assert!(validate_entry_path("").is_err());

        // Backslashes
        assert!(validate_entry_path("bms\\main.bms").is_err());
        assert!(validate_entry_path("audio\\sub\\01.wav").is_err());

        // Absolute paths
        assert!(validate_entry_path("/bms/main.bms").is_err());
        assert!(validate_entry_path("C:/foo.bms").is_err());
        assert!(validate_entry_path("D:/bms/main.bms").is_err());

        // Traversal
        assert!(validate_entry_path("../secret.txt").is_err());
        assert!(validate_entry_path("bms/../secret.txt").is_err());
        assert!(validate_entry_path("audio/sub/../../etc/passwd").is_err());

        // Ambiguous dot
        assert!(validate_entry_path("./bms/main.bms").is_err());
        assert!(validate_entry_path("bms/./main.bms").is_err());

        // Trailing / double slashes
        assert!(validate_entry_path("bms/").is_err());
        assert!(validate_entry_path("bms//main.bms").is_err());

        // Control characters
        assert!(validate_entry_path("bms/bad\0name.bms").is_err());
        assert!(validate_entry_path("bms/bad\nname.bms").is_err());
    }
}
